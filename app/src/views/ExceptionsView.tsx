import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Exceptions queue view (#332) — per-virksomhed kø af undtagelser (unmatched
// bank-rows, blokerede write-flows, dokumenter uden bilag-pligt-link osv.).
// Listen kommer fra det nye GET /api/companies/:slug/exceptions endpoint;
// POST .../exceptions/:id/resolve er allerede implementeret i kernen (#213,
// slice 1) og bruges af 'Marker som løst'-knappen.

import { useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { ApiError, api } from "../lib/api";
import type { CompanyExceptions, ExceptionRow } from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";

const STATUS_TABS: Array<{
	value: "open" | "resolved" | "all";
	label: string;
}> = [
	{ value: "open", label: "Åbne" },
	{ value: "resolved", label: "Løste" },
	{ value: "all", label: "Alle" },
];

const SEVERITY_LABEL: Record<ExceptionRow["severity"], string> = {
	high: "Høj",
	medium: "Medium",
	low: "Lav",
};

/**
 * Human-readable Danish labels for the raw `exception.type` codes the core
 * emits. Falls back to the raw code (in a <code>-tag) when an unknown type
 * shows up, so the screen never silently drops information. (Round-2 review:
 * "UNMATCHED_BANK_TRANSACTION raw code is shown to the user — should be
 * mapped to a Danish label.")
 */
const TYPE_LABEL: Record<string, string> = {
	UNMATCHED_BANK_TRANSACTION: "Ubehandlet banktransaktion",
	AGENT_NEEDS_REVIEW: "Agenten har brug for godkendelse",
	AGENT_PAYABLE_OVERDUE: "Forfalden leverandørpost (agent)",
	AGENT_ACCRUAL_READY: "Periodisering klar til bogføring (agent)",
	AGENT_ASSET_CANDIDATE: "Muligt anlæg over kapitaliseringsgrænsen (agent)",
	AGENT_TAX_NEEDS_REVIEW: "Oplysningsskema-felt skal kontrolleres (agent)",
	DOCUMENT_NO_BILAG: "Bilag mangler",
	PERIOD_LOCKED_WRITE: "Bogføring blokeret af periodelås",
	BACKUP_LOCKED_WRITE: "Bogføring blokeret af backup-lås",
};

export function ExceptionsView() {
	const { slug = "" } = useParams();
	const [params, setParams] = useSearchParams();
	const statusRaw = params.get("status") ?? "open";
	const status: "open" | "resolved" | "all" =
		statusRaw === "resolved" || statusRaw === "all" ? statusRaw : "open";
	// Resolve-tilstand: undgå at klikke flere gange på samme række.
	const [resolving, setResolving] = useState<Set<number>>(new Set());
	const [resolveError, setResolveError] = useState<string | null>(null);

	const state = useAsync<CompanyExceptions>(
		(signal) => api.exceptions(slug, status, { signal }),
		[slug, status],
	);

	const setStatus = (next: "open" | "resolved" | "all") => {
		const updated = new URLSearchParams(params);
		if (next === "open") updated.delete("status");
		else updated.set("status", next);
		setParams(updated, { replace: true });
	};

	const outcome = useMutationOutcome(state.reload);
	const resolve = async (row: ExceptionRow) => {
		if (resolving.has(row.id) || outcome.isBlocked()) return;
		setResolveError(null);
		setResolving((s) => new Set([...s, row.id]));
		try {
			await outcome.run(() =>
				api.resolveException(slug, row.id, "Markeret som løst fra cockpittet"),
			);
			state.reload();
		} catch (err) {
			setResolveError(
				err instanceof ApiError ? err.message : "Kunne ikke markere som løst.",
			);
		} finally {
			setResolving((s) => {
				const next = new Set(s);
				next.delete(row.id);
				return next;
			});
		}
	};

	// Keep stale data visible during a reload (matches DashboardView/InvoicesView/
	// BankView) — only show the spinner on the FIRST load, never on a refresh.
	if (state.loading && !state.data) return <Loading />;
	// `onRetry` so a failed load is not a dead end — the owner can re-run it.
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;
	const data = state.data!;
	const rows = data.rows;

	return (
		<section
			data-cockpit-page="exceptions"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			{outcome.feedback}
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
					Status kunne ikke opdateres. De tidligere hentede undtagelser vises
					fortsat.
				</div>
			)}
			<PageHeader
				title="Undtagelser"
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
								xstyle={[cockpitStyles.exceptionsViewBtnComposition]}
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
						{data.company.country} · Undtagelser
					</p>
				</div>
			</PageHeader>

			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Status
				</h3>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.filterBar,
					)}
				>
					{STATUS_TABS.map((t) => (
						<Button
							key={t.value}
							aria-pressed={status === t.value}
							type="button"
							onClick={() => setStatus(t.value)}
							variant={!(status === t.value) ? "secondary" : "primary"}
							xstyle={[cockpitStyles.exceptionsViewBtnComposition2]}
						>
							{t.label}
						</Button>
					))}
				</div>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{data.count} undtagelse{data.count === 1 ? "" : "r"}
					{status === "open" && (
						<>
							{" "}
							· Høj: {data.bySeverity.high} · Medium: {data.bySeverity.medium} ·
							Lav: {data.bySeverity.low}
						</>
					)}
				</p>
			</section>

			{resolveError && (
				<div
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{resolveError}
				</div>
			)}

			{rows.length === 0 ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen undtagelser i denne status.
					</p>
				</div>
			) : (
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
							cockpitStyles.exceptionsViewTableTable,
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
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									ID
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									Type
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									Alvor
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									Status
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									Besked
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									Næste skridt
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
									)}
								>
									Oprettet
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.exceptionsViewTableTableThLastChild,
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
							{rows.map((row) => (
								<tr
									key={row.id}
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
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										#{row.id}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{TYPE_LABEL[row.type] ?? (
											<code
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.code,
												)}
											>
												{row.type}
											</code>
										)}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{SEVERITY_LABEL[row.severity]}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{row.status === "open" ? "Åben" : "Løst"}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{row.message}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{row.requiredAction ?? "—"}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{row.createdAt}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.exceptionsViewTableTableTdLastChild,
										)}
									>
										{row.status === "open" ? (
											<Button
												requiredPermission="company.review"
												variant="secondary"
												type="button"
												onClick={() => resolve(row)}
												disabled={resolving.has(row.id) || outcome.blocked}
												xstyle={[cockpitStyles.exceptionsViewBtnComposition2]}
											>
												{resolving.has(row.id)
													? "Markerer …"
													: "Markér som løst"}
											</Button>
										) : (
											<span
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.muted,
												)}
											>
												Løst{row.resolvedAt ? ` ${row.resolvedAt}` : ""}
												{row.resolvedBy ? ` af ${row.resolvedBy}` : ""}
											</span>
										)}
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}
		</section>
	);
}
