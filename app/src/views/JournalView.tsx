import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Posteringer — the per-company journal (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/journal?year=`: the posted journal entries for
// the year (entry no, date, text, total). Clicking an entry expands it to show
// its debit/credit lines (account no + name, debit, credit). All money fields
// are kroner — `formatKroner` is used throughout.
//
// #396 — filter-bar: fritekstsøgning (entry-tekst, linje-tekst, bilagsnummer,
// modkonto), datointerval og beløbsspand. Alle filtre er client-side og
// afspejles i URL-params (`q`, `from`, `to`, `amountMin`, `amountMax`) så
// ejeren kan dele linket eller komme tilbage til samme udsnit.

import { useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { ArchivedBanner } from "../components/ArchivedBanner";
import {
	FilterBar,
	FormField,
	PageState,
} from "../components/CockpitPrimitives";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PartyLink } from "../components/PartyLink";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyJournal, JournalEntry } from "../lib/types";
// #379 — the EntryRow needs the slug to build the bilag-file URL on the fly.
import { useAsync } from "../lib/useAsync";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";

const FILTER_PARAM_KEYS = [
	"q",
	"from",
	"to",
	"amountMin",
	"amountMax",
	"journalEntryId",
	"journalLineId",
] as const;

export function JournalView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	// An optional account drill-down: `?account=<accountNo>` filters the journal
	// to the entries that touch that account (set by the statement views).
	const [params, setParams] = useSearchParams();
	const [page, setPage] = useState(0);
	const [selectedEntryId, setSelectedEntryId] = useState<number | null>(null);
	const account = params.get("account") ?? undefined;
	const journalEntryId = Number(params.get("journalEntryId")) || null;
	const journalLineId = Number(params.get("journalLineId")) || null;
	const clearAccount = () => {
		const next = new URLSearchParams(params);
		next.delete("account");
		setParams(next, { replace: true });
	};

	// --- #396 filter-bar params (client-side; reflected in URL) ---------------
	const q = params.get("q") ?? "";
	const fromDate = params.get("from") ?? "";
	const toDate = params.get("to") ?? "";
	const amountMin = params.get("amountMin") ?? "";
	const amountMax = params.get("amountMax") ?? "";

	function setFilter(key: (typeof FILTER_PARAM_KEYS)[number], value: string) {
		const next = new URLSearchParams(params);
		if (value === "") {
			next.delete(key);
		} else {
			next.set(key, value);
		}
		setParams(next, { replace: true });
	}

	function clearAllFilters() {
		const next = new URLSearchParams(params);
		for (const k of FILTER_PARAM_KEYS) next.delete(k);
		setParams(next, { replace: true });
	}

	const hasActiveFilter =
		q !== "" ||
		fromDate !== "" ||
		toDate !== "" ||
		amountMin !== "" ||
		amountMax !== "" ||
		journalEntryId !== null ||
		journalLineId !== null;

	const state = useAsync<CompanyJournal>(
		(signal) => api.journal(slug, year, account, { signal }),
		[slug, year, account],
	);

	const filteredEntries = useMemo(() => {
		const entries = state.data?.entries ?? [];
		if (!hasActiveFilter) return entries;
		const needle = q.trim().toLowerCase();
		const minN = amountMin === "" ? null : Number(amountMin);
		const maxN = amountMax === "" ? null : Number(amountMax);
		return entries.filter((entry) => {
			if (journalEntryId !== null && entry.id !== journalEntryId) return false;
			if (
				journalLineId !== null &&
				!entry.lines.some((line) => line.journalLineId === journalLineId)
			)
				return false;
			if (needle !== "" && !entryMatchesText(entry, needle)) return false;
			if (fromDate !== "" && entry.date < fromDate) return false;
			if (toDate !== "" && entry.date > toDate) return false;
			if (minN !== null && !Number.isNaN(minN) && entry.total < minN)
				return false;
			if (maxN !== null && !Number.isNaN(maxN) && entry.total > maxN)
				return false;
			return true;
		});
	}, [
		state.data,
		hasActiveFilter,
		q,
		fromDate,
		toDate,
		amountMin,
		amountMax,
		journalEntryId,
		journalLineId,
	]);

	if (state.loading && !state.data)
		return (
			<section
				data-evidence-issue="652"
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
					Posteringer
				</h2>
				<p
					data-evidence-status="loading"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Henter posteringer
				</p>
				<PageState kind="loading" title="Henter posteringer" />
			</section>
		);
	if (state.error)
		return (
			<section
				data-evidence-issue="652"
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
					Posteringer
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
						? "Postering kræver afklaring"
						: "Posteringer kunne ikke hentes"}
				</p>
				<PageState
					kind="error"
					title="Posteringer kunne ikke hentes"
					onRetry={state.reload}
				>
					{state.error}
				</PageState>
			</section>
		);

	const j = state.data!;
	const currency = j.company.currency || "DKK";
	const totalCount = j.entries.length;
	const matchCount = filteredEntries.length;
	const pageSize = 25;
	const pageEntries = filteredEntries.slice(
		page * pageSize,
		page * pageSize + pageSize,
	);

	return (
		<section
			data-cockpit-page="journal"
			data-evidence-issue="652"
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
					Status kunne ikke opdateres. De tidligere hentede oplysninger vises
					fortsat.
				</div>
			)}
			<PageHeader
				evidenceHeading
				title="Posteringer"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							{/* #465 — revisor-anmodning: hele kassekladden som CSV. URL'en
              bærer den aktive konto-drilldown med, så ejeren kan eksportere
              "kun denne konto"-udsnittet direkte. */}
							<a
								href={api.journalCsvUrl(slug, j.selectedYear, account ?? null)}
								download
								{...stylex.props(cockpitStyles.statementBtnComposition2)}
							>
								Hent CSV
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
						data-evidence-status={filteredEntries.length ? "normal" : "empty"}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{filteredEntries.length
							? "Posteringer klar"
							: "Ingen posteringer i perioden"}
					</p>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{j.company.cvr ? `CVR ${j.company.cvr} · ` : ""}
						{j.company.country} · {currency} · Posteringer
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={j.fiscalYears}
				selectedYear={j.selectedYear}
				onYearChange={setYear}
			/>

			{j.archived && (
				<ArchivedBanner year={j.selectedYear} source={j.archivedSource} />
			)}
			{j.accountFilter && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.accountFilter,
					)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.accountFilterP,
						)}
					>
						Posteringer på konto{" "}
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{j.accountFilter.accountNo}
						</span>{" "}
						{j.accountFilter.name}
					</p>
					<Button
						variant="secondary"
						type="button"
						onClick={clearAccount}
						xstyle={[cockpitStyles.statementBtnComposition]}
					>
						Vis alle posteringer
					</Button>
				</div>
			)}

			<FilterBar
				activeFilters={
					[
						q && `Søgning: ${q}`,
						fromDate && `Fra: ${fromDate}`,
						toDate && `Til: ${toDate}`,
						amountMin && `Beløb fra: ${amountMin}`,
						amountMax && `Beløb til: ${amountMax}`,
					].filter(Boolean) as string[]
				}
				onReset={clearAllFilters}
				resetLabel="Ryd filtre"
				advanced={
					<>
						<FormField
							label="Beløb min"
							xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
						>
							<Input
								type="number"
								inputMode="decimal"
								value={amountMin}
								placeholder="0"
								onChange={(e) => setFilter("amountMin", e.target.value)}
								xstyle={[cockpitStyles.statementInputComposition]}
							/>
						</FormField>
						<FormField
							label="Beløb maks"
							xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
						>
							<Input
								type="number"
								inputMode="decimal"
								value={amountMax}
								placeholder="∞"
								onChange={(e) => setFilter("amountMax", e.target.value)}
								xstyle={[cockpitStyles.statementInputComposition]}
							/>
						</FormField>
					</>
				}
			>
				<FormField
					label="Søg"
					xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
				>
					<Input
						type="search"
						value={q}
						placeholder="Søg på tekst, bilagsnummer eller konto…"
						onChange={(e) => setFilter("q", e.target.value)}
						xstyle={[cockpitStyles.statementInputComposition]}
					/>
				</FormField>
				<FormField
					label="Fra"
					xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
				>
					<Input
						type="date"
						value={fromDate}
						onChange={(e) => setFilter("from", e.target.value)}
						xstyle={[cockpitStyles.statementInputComposition]}
					/>
				</FormField>
				<FormField
					label="Til"
					xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
				>
					<Input
						type="date"
						value={toDate}
						onChange={(e) => setFilter("to", e.target.value)}
						xstyle={[cockpitStyles.statementInputComposition]}
					/>
				</FormField>
			</FilterBar>
			<details
				data-evidence-progressive
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Forklar posten
				</summary>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Åbn en postering for at se kontering og dokumenteret grundlag.
				</p>
			</details>

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}
			>
				{j.periodStart} – {j.periodEnd} ·{" "}
				{hasActiveFilter
					? `${matchCount} af ${totalCount} posteringer matcher`
					: `${totalCount} posteringer`}
			</p>
			{filteredEntries.length === 0 ? (
				<PageState
					kind="empty"
					title={
						hasActiveFilter ? "Ingen posteringer" : "Ingen posteringer i året"
					}
				>
					{hasActiveFilter
						? "Ingen posteringer matcher filtrene."
						: j.accountFilter
							? "Ingen posteringer på kontoen i året."
							: "Ingen posteringer i året."}
				</PageState>
			) : (
				<ul
					aria-label="Posteringer"
					data-evidence-data
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.entryList,
					)}
				>
					{pageEntries.map((entry) => (
						<EntryRow
							key={entry.id}
							entry={entry}
							currency={currency}
							slug={slug}
							archived={j.archived}
							open={selectedEntryId === entry.id}
							onSelect={() =>
								setSelectedEntryId((current) =>
									current === entry.id ? null : entry.id,
								)
							}
						/>
					))}
				</ul>
			)}
			{filteredEntries.length > pageSize && (
				<nav
					aria-label="Sider"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<button
						type="button"
						disabled={page === 0}
						onClick={() => setPage((p) => p - 1)}
						{...stylex.props(cockpitStyles.statementBtnComposition)}
					>
						Forrige
					</button>
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Side {page + 1} af {Math.ceil(filteredEntries.length / pageSize)}
					</span>
					<button
						type="button"
						disabled={(page + 1) * pageSize >= filteredEntries.length}
						onClick={() => setPage((p) => p + 1)}
						{...stylex.props(cockpitStyles.statementBtnComposition)}
					>
						Næste
					</button>
				</nav>
			)}
		</section>
	);
}

function entryMatchesText(entry: JournalEntry, needle: string): boolean {
	if (entry.entryNo.toLowerCase().includes(needle)) return true;
	if (entry.text.toLowerCase().includes(needle)) return true;
	for (const line of entry.lines) {
		if (line.accountNo.toLowerCase().includes(needle)) return true;
		if (line.accountName.toLowerCase().includes(needle)) return true;
		if (line.text && line.text.toLowerCase().includes(needle)) return true;
	}
	return false;
}

function EntryRow({
	entry,
	currency,
	slug,
	archived,
	open,
	onSelect,
}: {
	entry: JournalEntry;
	currency: string;
	slug: string;
	/**
	 * #379 — arkiverede regnskabsår har ingen bilag-linkage; vis "Intet bilag"
	 * også når posten i teorien havde en `documentId`, sådan at vi aldrig
	 * sender ejeren mod en route der ikke kan resolves.
	 */
	archived: boolean;
	open: boolean;
	onSelect: () => void;
}) {
	// #379 — en post har et bilag når både linkage og fil-route er meningsfulde.
	// Arkiverede år vises altid som "Intet bilag" (filen er ikke i `documents`).
	const hasDocument = !archived && entry.documentId !== null;
	return (
		<li
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.entryItem,
				open && cockpitStyles.entryItemOpen,
			)}
		>
			<Button
				type="button"
				aria-expanded={open}
				data-evidence-core-action
				onClick={onSelect}
				xstyle={[cockpitStyles.entrySummaryComposition]}
			>
				<span
					aria-hidden="true"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.entryCaret,
					)}
				>
					{open ? "▾" : "▸"}
				</span>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.entryNo,
					)}
				>
					{entry.entryNo}
				</span>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.entryDate,
						cockpitStyles.entrySummaryEntryDate,
					)}
				>
					{entry.date}
				</span>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.entryText,
					)}
				>
					<PartyLink slug={slug} partyId={entry.partyId}>
						{entry.text}
					</PartyLink>
				</span>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{entry.documentNo ? "Bilag" : "Bilag mangler"}
				</span>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Bogført
				</span>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.num,
						cockpitStyles.entrySummaryEntryTotal,
					)}
				>
					{formatKroner(entry.total, currency)}
				</span>
			</Button>
			{open && (
				<p
					data-evidence-task-outcome
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Forklaring åbnet
				</p>
			)}
			{open && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.tableScroll,
						cockpitStyles.entryLines,
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
									scope="col"
									{...stylex.props(
										cockpitStyles.entryLinesTableStatementTableThComposition,
									)}
								>
									Konto
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.entryLinesTableStatementTableThComposition,
									)}
								>
									Navn
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.entryLinesTableStatementTableThComposition2,
									)}
								>
									Debet
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.entryLinesTableStatementTableThComposition2,
									)}
								>
									Kredit
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.entryLinesTableStatementTableThComposition,
									)}
								>
									Dimensioner
								</th>
							</tr>
						</thead>
						<tbody
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{entry.lines.map((line, i) => (
								<tr
									key={i}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<td
										{...stylex.props(
											cockpitStyles.entryLinesTableStatementTableTdComposition,
										)}
									>
										{line.accountNo}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.entryLinesTableStatementTableTdComposition2,
										)}
									>
										{line.accountName}
										{line.text ? (
											<span
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.muted,
												)}
											>
												{" "}
												· {line.text}
											</span>
										) : null}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.entryLinesTableStatementTableTdComposition3,
										)}
									>
										{line.debit ? formatKroner(line.debit, currency) : "—"}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.entryLinesTableStatementTableTdComposition3,
										)}
									>
										{line.credit ? formatKroner(line.credit, currency) : "—"}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.entryLinesTableStatementTableTdComposition2,
										)}
									>
										{line.journalLineId === null ? (
											<span
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.muted,
												)}
											>
												Ingen dimensionshistorik
											</span>
										) : (
											<DimensionAssignments
												slug={slug}
												journalLineId={line.journalLineId}
											/>
										)}
									</td>
								</tr>
							))}
						</tbody>
					</table>
					<div
						data-evidence-progressive
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<ExplanationPanel slug={slug} entryId={entry.id} />
						{hasDocument ? (
							<a
								href={api.documentFileUrl(slug, entry.documentId!)}
								target="_blank"
								rel="noreferrer"
								{...stylex.props(cockpitStyles.aComposition)}
							>
								Åbn bilag
								{entry.documentNo ? (
									<span
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{" "}
										· {entry.documentNo}
									</span>
								) : null}
							</a>
						) : (
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Intet bilag
							</span>
						)}
					</div>
				</div>
			)}
		</li>
	);
}

function ExplanationPanel({
	slug,
	entryId,
}: {
	slug: string;
	entryId: number;
}) {
	const state = useAsync<any>(
		() => api.journalExplanation(slug, entryId),
		[slug, entryId],
	);
	if (state.loading && !state.data)
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Henter forklaring…
			</span>
		);
	if (state.error)
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Forklaringen kunne ikke hentes.
			</span>
		);
	const e = state.data;
	return (
		<details
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<summary {...stylex.props(cockpitStyles.summaryComposition)}>
				Forklar posten
			</summary>
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				Konkrete kilder vises kun, når de er eksplicit knyttet til posteringen.
			</p>
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<strong
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Faglig vurdering:
				</strong>{" "}
				{e.professionalAssessment.text}
			</p>
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<strong
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Lovgrundlag:
				</strong>{" "}
				{e.legalSource.text}
			</p>
			{e.appliedRule && (
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Anvendt Rentemester-regel:
					</strong>{" "}
					{e.appliedRule.ruleId} v{e.appliedRule.version} (gælder fra{" "}
					{e.appliedRule.effectiveFrom}).
				</p>
			)}
			{e.correction.sentence && (
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{e.correction.sentence}
				</p>
			)}
			<details
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Evidens
				</summary>
				<code
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}
				>
					{e.evidence.entryHash}
				</code>
			</details>
		</details>
	);
}

type Allocation = {
	dimensionId: string;
	memberId: string;
	amountMinor: number;
	currency: string;
};
type AssignmentEvent = {
	id: number;
	allocations_json: string;
	source: string;
	source_ref: string | null;
	plan_hash: string;
	event_type: "assigned" | "superseded";
	supersedes_assignment_id: number | null;
	actor: string;
	principal: string;
	created_at: string;
};

function parseAllocations(value: string): Allocation[] {
	try {
		const parsed: unknown = JSON.parse(value);
		if (!Array.isArray(parsed)) return [];
		return parsed.filter(
			(item): item is Allocation =>
				Boolean(item) &&
				typeof item === "object" &&
				typeof (item as Allocation).dimensionId === "string" &&
				typeof (item as Allocation).memberId === "string" &&
				Number.isSafeInteger((item as Allocation).amountMinor) &&
				typeof (item as Allocation).currency === "string",
		);
	} catch {
		return [];
	}
}

/** Current means an assigned event that no later supersession explicitly retires. */
function currentAssignment(events: AssignmentEvent[]): AssignmentEvent | null {
	const retired = new Set(
		events
			.map((event) => event.supersedes_assignment_id)
			.filter((id): id is number => id !== null),
	);
	return (
		events.find(
			(event) => event.event_type === "assigned" && !retired.has(event.id),
		) ?? null
	);
}

function DimensionAssignments({
	slug,
	journalLineId,
}: {
	slug: string;
	journalLineId: number;
}) {
	const state = useAsync<AssignmentEvent[]>(
		(signal) =>
			api.dimensionAssignments(slug, journalLineId, { signal }) as Promise<
				AssignmentEvent[]
			>,
		[slug, journalLineId],
	);
	if (state.loading && !state.data)
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Henter dimensioner…
			</span>
		);
	if (state.error && !state.data)
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Dimensionshistorik kunne ikke hentes
			</span>
		);
	const events = state.data ?? [];
	const current = currentAssignment(events);
	if (!current)
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Ingen godkendt dimension
			</span>
		);
	const allocations = parseAllocations(current.allocations_json);
	return (
		<details
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			{state.error && (
				<p
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Dimensionshistorik kunne ikke opdateres. Den tidligere hentede
					historik vises fortsat.
				</p>
			)}
			<summary {...stylex.props(cockpitStyles.summaryComposition)}>
				{allocations
					.map((item) => `${item.dimensionId}: ${item.memberId}`)
					.join(", ") || "Godkendt dimension"}
			</summary>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Kilde: {current.source} · plan-hash{" "}
				<code
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}
				>
					{current.plan_hash}
				</code>
				<br
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				/>
				{current.source_ref && (
					<>
						Kildereference: {current.source_ref}
						<br
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						/>
					</>
				)}
				Godkendt af {current.actor} via {current.principal} ·{" "}
				{current.created_at}
			</p>
			<DimensionReview
				slug={slug}
				journalLineId={journalLineId}
				current={current}
				onChanged={state.reload}
			/>
		</details>
	);
}

/**
 * The legal posting is immutable. A correction therefore first retires the
 * current allocation and only then applies the exact hash-bound replacement.
 * This compact form intentionally accepts only explicit allocation ids and
 * amounts — it does not guess a dimension or silently redistribute amounts.
 */
function DimensionReview({
	slug,
	journalLineId,
	current,
	onChanged,
}: {
	slug: string;
	journalLineId: number;
	current: AssignmentEvent;
	onChanged: () => void;
}) {
	const [editing, setEditing] = useState(false);
	const [allocations, setAllocations] = useState<Allocation[]>(() =>
		parseAllocations(current.allocations_json),
	);
	const [plan, setPlan] = useState<{ planHash: string } | null>(null);
	const [reviewed, setReviewed] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [confirmSupersede, setConfirmSupersede] = useState(false);
	const outcome = useMutationOutcome(onChanged);
	const guard = useDiscardGuard(
		editing &&
			JSON.stringify(allocations) !==
				JSON.stringify(parseAllocations(current.allocations_json)),
		() => {
			setEditing(false);
			setPlan(null);
			setReviewed(false);
			setConfirmSupersede(false);
			setAllocations(parseAllocations(current.allocations_json));
		},
	);

	function change(index: number, key: keyof Allocation, value: string) {
		setPlan(null);
		setReviewed(false);
		setAllocations((rows) =>
			rows.map((row, i) =>
				i === index
					? { ...row, [key]: key === "amountMinor" ? Number(value) : value }
					: row,
			),
		);
	}
	async function makePlan() {
		if (outcome.isBlocked()) return;
		setError(null);
		try {
			const result = await api.planDimensionAssignment(slug, {
				journalLineId,
				allocations,
				source: "reviewed",
			});
			if (!result.ok || !result.plan)
				throw new Error(
					result.errors?.join(", ") || "Planen kunne ikke valideres.",
				);
			setPlan(result.plan);
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : String(cause));
		}
	}
	async function replace(reason: string) {
		if (outcome.isBlocked()) return;
		if (!reason.trim())
			throw new Error("Skriv en begrundelse for korrektionen.");
		if (!plan || !reviewed) throw new Error("Gennemgå først den præcise plan.");
		await outcome.run(() =>
			api.replaceDimensionAssignment(slug, {
				journalLineId,
				expectedAssignmentId: current.id,
				allocations,
				source: "reviewed",
				planHash: plan.planHash,
				reason,
				idempotencyKey: `cockpit-dimension-replace-${current.id}-${plan.planHash}`,
			}),
		);
		guard.dismiss();
		onChanged();
	}
	if (!editing)
		return (
			<>
				{outcome.feedback}
				<Button
					requiredPermission="company.review"
					disabled={outcome.blocked}
					variant="secondary"
					type="button"
					onClick={() => setEditing(true)}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Gennemgå og ret
				</Button>
			</>
		);
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
			)}
		>
			{outcome.feedback}
			{guard.confirmation}
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<strong
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Ret dimensionsklassifikation
				</strong>
			</p>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Posteringen ændres aldrig. Den nuværende tildeling supersederes og den
				reviewede plan anvendes atomisk med begrundelse.
			</p>
			{allocations.map((allocation, index) => (
				<div
					key={`${allocation.dimensionId}-${index}`}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Dimension
						<Input
							disabled={outcome.blocked}
							aria-label={`Dimension ${index + 1}`}
							value={allocation.dimensionId}
							onChange={(event) =>
								change(index, "dimensionId", event.target.value)
							}
							xstyle={[cockpitStyles.rowActionsInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Medlem
						<Input
							disabled={outcome.blocked}
							aria-label={`Medlem ${index + 1}`}
							value={allocation.memberId}
							onChange={(event) =>
								change(index, "memberId", event.target.value)
							}
							xstyle={[cockpitStyles.rowActionsInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Øre
						<Input
							disabled={outcome.blocked}
							aria-label={`Øre ${index + 1}`}
							type="number"
							value={allocation.amountMinor}
							onChange={(event) =>
								change(index, "amountMinor", event.target.value)
							}
							xstyle={[cockpitStyles.rowActionsInputComposition]}
						/>
					</label>
				</div>
			))}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.rowActions,
				)}
			>
				<Button
					variant="secondary"
					type="button"
					disabled={outcome.blocked}
					onClick={() => void makePlan()}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Validér revideret plan
				</Button>
				<Button
					variant="secondary"
					type="button"
					onClick={guard.onClose}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Annullér
				</Button>
			</div>
			{plan && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Plan-hash:{" "}
						<code
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.code,
							)}
						>
							{plan.planHash}
						</code>
					</p>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<Input
							disabled={outcome.blocked}
							type="checkbox"
							checked={reviewed}
							onChange={(event) => setReviewed(event.target.checked)}
							xstyle={[cockpitStyles.inputComposition]}
						/>{" "}
						Jeg har gennemgået den præcise plan.
					</label>
					<Button
						variant="secondary"
						type="button"
						requiredPermission="company.review"
						disabled={!reviewed || outcome.blocked}
						onClick={() => setConfirmSupersede(true)}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Erstat nuværende tildeling atomisk
					</Button>
				</div>
			)}
			{error && (
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
			)}
			{confirmSupersede && (
				<ConfirmDialog
					title="Erstat dimensionsklassifikation"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Den nuværende klassifikation bevares i revisionssporet.
							Supersession og den præcise, hash-bundne erstatning gemmes
							atomisk, så linjen aldrig står uden en aktuel tildeling.
						</p>
					}
					confirmLabel="Erstat tildeling"
					confirmKind="danger"
					noteLabel="Begrundelse"
					onConfirm={replace}
					onClose={() => setConfirmSupersede(false)}
					onRefresh={onChanged}
				/>
			)}
		</div>
	);
}
