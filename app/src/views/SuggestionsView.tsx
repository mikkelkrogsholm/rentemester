import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Agent-forslag → menneskelig godkendelse-flow (#346).
//
// Rentemester's narrative is: agent surfaces, human decides, ledger enforces.
// The agent loop and the exception sync functions raise `AGENT_*` exceptions
// whenever a deterministic agent run needs the owner's call — an overdue
// kreditorpost, a periodeafgrænsnings-periode der er klar til bogføring, et
// muligt anlæg over kapitaliseringsgrænsen, et needs-review-punkt på
// oplysningsskemaet. The dashboard already shows them collapsed as a count;
// this view makes them individually visible and gives the owner two explicit
// actions per row:
//
//   * "Godkend" — accepts the suggestion. The view resolves the underlying
//     exception with a "Godkendt af ejer i cockpit"-note, then deep-links to
//     the action-specific view (Anlæg, Leverandørfaktura, Posteringer) where
//     the owner actually books the entry. Approve here NEVER posts on its own.
//
//   * "Afvis" — rejects the suggestion with a free-text reason. The exception
//     is resolved with an "Afvist af ejer i cockpit"-note carrying the reason
//     so the audit trail preserves WHY the owner declined.
//
// All approvals and rejections go through the SAME `resolveException` core
// that the existing "Løs"-button uses, so the audit chain stays intact. The
// cockpit never re-implements the underlying bookkeeping.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { ApiError, api } from "../lib/api";
import type { AgentSuggestionRow, CompanyAgentSuggestions } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const SEVERITY_LABEL: Record<AgentSuggestionRow["severity"], string> = {
	high: "Høj prioritet",
	medium: "Mellem prioritet",
	low: "Lav prioritet",
};

export function SuggestionsView() {
	const { slug = "" } = useParams();
	const state = useAsync<CompanyAgentSuggestions>(
		(signal) => api.agentSuggestions(slug, { signal }),
		[slug],
	);
	const [actionError, setActionError] = useState<string | null>(null);

	if (state.loading && !state.data)
		return <Loading label="Henter agent-forslag…" />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const data = state.data!;
	const currency = data.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="suggestions"
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
					Status kunne ikke opdateres. De tidligere hentede oplysninger vises
					fortsat.
				</div>
			)}
			<PageHeader
				title="Forslag"
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
						{data.company.country} · {currency} · Agent-forslag
					</p>
				</div>
			</PageHeader>

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}
			>
				Agenten foreslår — du beslutter. Hvert forslag er deterministisk afledt
				af en regel i{" "}
				<code
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}
				>
					rules/dk/*.yaml
				</code>{" "}
				og bogføres aldrig uden et eksplicit klik fra dig. Godkendelse løser
				forslaget; den konkrete postering laver du på den linkede side.
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
						Forslag i kø
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{data.count}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						{data.count === 0
							? "Agenten har intet, der venter på en beslutning."
							: data.count === 1
								? "Ét forslag venter på din beslutning."
								: `${data.count} forslag venter på din beslutning.`}
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
						Høj prioritet
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{data.bySeverity.high}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						Bør afgøres først — kan udløse rentepåkrav eller manglende
						bogføring.
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
						Mellem / Lav
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{data.bySeverity.medium + data.bySeverity.low}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						{data.bySeverity.medium} mellem · {data.bySeverity.low} lav
					</p>
				</div>
			</div>

			{actionError ? (
				<div
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{actionError}
					</p>
				</div>
			) : null}

			<h3
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					viewStyles.site0,
				)}
			>
				Forslag
			</h3>
			{data.rows.length === 0 ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Agenten har ingen åbne forslag. Hver gang en automatisk kørsel løber
						ind i en sag, der kræver din vurdering — fx en overforfalden
						kreditorpost, et muligt anlæg, eller en periodeafgrænsning klar til
						bogføring — dukker forslaget op her.
					</p>
				</div>
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
									Prioritet
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}
								>
									Type
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}
								>
									Agentens vurdering
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}
								>
									Hjemmel
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
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
							{data.rows.map((row) => (
								<SuggestionRowView
									key={row.exceptionId}
									row={row}
									slug={slug}
									onChanged={() => state.reload()}
									onError={setActionError}
								/>
							))}
						</tbody>
					</table>
				</div>
			)}

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.statementCheck,
					cockpitStyles.statementCheckOk,
				)}
			>
				Hvert forslag er en åben undtagelse i ledger'en — godkendelse og
				afvisning løser undtagelsen via samme kerne (
				<code
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}
				>
					resolveException
				</code>
				) som CLI'en og MCP-kald bruger, så beslutningen står på audit-sporet
				med din actor og en beslutningstekst.
			</p>
		</section>
	);
}

function SuggestionRowView({
	row,
	slug,
	onChanged,
	onError,
}: {
	row: AgentSuggestionRow;
	slug: string;
	onChanged: () => void;
	onError: (msg: string) => void;
}) {
	const [busy, setBusy] = useState<"approve" | "reject" | null>(null);
	const [pending, setPending] = useState<"approve" | "reject" | null>(null);

	async function doApprove() {
		setBusy("approve");
		try {
			await api.approveAgentSuggestion(slug, row.exceptionId);
			onChanged();
		} catch (err) {
			onError(
				err instanceof ApiError
					? err.message
					: "Kunne ikke godkende forslaget.",
			);
			throw err;
		} finally {
			setBusy(null);
		}
	}

	async function doReject(note: string) {
		setBusy("reject");
		try {
			await api.rejectAgentSuggestion(
				slug,
				row.exceptionId,
				note.length > 0 ? note : undefined,
			);
			onChanged();
		} catch (err) {
			onError(
				err instanceof ApiError ? err.message : "Kunne ikke afvise forslaget.",
			);
			throw err;
		} finally {
			setBusy(null);
		}
	}

	const severityLabel = SEVERITY_LABEL[row.severity];
	const flagClass =
		row.severity === "high"
			? "warn"
			: row.severity === "medium"
				? "neutral"
				: "ok";

	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.flag,
						flagClass === "ok" && cockpitStyles.flagOk,
						flagClass === "neutral" && cockpitStyles.flagNeutral,
					)}
				>
					{severityLabel}
				</span>
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<strong
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{row.kindLabel}
				</strong>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
						viewStyles.site1,
					)}
				>
					{row.type}
					{row.agentActor ? ` · ${row.agentActor}` : ""}
				</div>
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						viewStyles.site2,
					)}
				>
					{row.rationale}
				</p>
				{row.requiredAction ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							viewStyles.site3,
						)}
					>
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Foreslået handling:
						</strong>{" "}
						{row.requiredAction}
					</p>
				) : null}
				{row.link ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							viewStyles.site4,
						)}
					>
						<Link
							to={`/companies/${slug}/${row.link}`}
							{...stylex.props(cockpitStyles.aComposition)}
						>
							Åbn relateret side →
						</Link>
					</p>
				) : null}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.ruleId ? (
					<code
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}
					>
						{row.ruleId}
					</code>
				) : (
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						—
					</span>
				)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<Button
						requiredPermission="company.review"
						type="button"
						onClick={() => setPending("approve")}
						disabled={busy !== null}
						aria-label={`Godkend ${row.kindLabel}`}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{busy === "approve" ? "Godkender…" : "Godkend"}
					</Button>
					<Button
						requiredPermission="company.review"
						variant="secondary"
						type="button"
						onClick={() => setPending("reject")}
						disabled={busy !== null}
						aria-label={`Afvis ${row.kindLabel}`}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{busy === "reject" ? "Afviser…" : "Afvis"}
					</Button>
				</div>
				{pending === "approve" && (
					<ConfirmDialog
						title={`Godkend forslag: ${row.kindLabel}`}
						body={
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Forslaget løses som godkendt — du bogfører selv den konkrete
								handling bagefter. Godkendelsen gemmes på revisionssporet.
							</p>
						}
						confirmLabel="Godkend"
						onConfirm={async () => {
							await doApprove();
						}}
						onClose={() => setPending(null)}
						onRefresh={onChanged}
					/>
				)}
				{pending === "reject" && (
					<ConfirmDialog
						title={`Afvis forslag: ${row.kindLabel}`}
						body={
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Skriv eventuelt en kort begrundelse — den gemmes på
								revisionssporet, så agenten kan lære af afvisningen. Du kan også
								lade feltet stå tomt.
							</p>
						}
						confirmLabel="Afvis forslag"
						confirmKind="danger"
						noteLabel="Begrundelse (valgfri)"
						notePlaceholder="fx 'ikke et nyt anlæg — direkte i drift'"
						onConfirm={async (note) => {
							await doReject(note);
						}}
						onClose={() => setPending(null)}
						onRefresh={onChanged}
					/>
				)}
			</td>
		</tr>
	);
}

const viewStyles = stylex.create({
	site0: { marginTop: "1.5rem" },
	site1: { fontSize: "0.85em" },
	site2: { margin: 0 },
	site3: { margin: "0.25rem 0 0 0", fontSize: "0.9em" },
	site4: { margin: "0.25rem 0 0 0", fontSize: "0.9em" },
});
