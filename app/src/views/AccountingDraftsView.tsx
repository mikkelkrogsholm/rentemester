import * as stylex from "@stylexjs/stylex";
import { type FormEvent, useState } from "react";
import { useParams } from "react-router-dom";
import {
	FilterBar,
	PageState,
	ResponsiveTable,
} from "../components/CockpitPrimitives";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import {
	Button,
	ButtonLink,
	Input,
	PageHeader,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { ApiError, api } from "../lib/api";
import { formatKroner, parseDanishAmount, todayIso } from "../lib/format";
import type {
	AccountingDraft,
	AccountingDraftLine,
	AccountingDraftPayload,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

type LineForm = {
	accountNo: string;
	debitAmount: string;
	creditAmount: string;
	vatCode: string;
	text: string;
};
const EMPTY_LINE: LineForm = {
	accountNo: "",
	debitAmount: "",
	creditAmount: "",
	vatCode: "",
	text: "",
};
const STATUS_LABEL: Record<AccountingDraft["status"], string> = {
	created: "Kladde",
	revised: "Revideret",
	submitted: "Afventer godkendelse",
	rejected: "Afvist",
	approved_posted: "Godkendt og bogført",
};

function optionalPositiveInteger(value: string): number | undefined {
	if (!value.trim()) return undefined;
	const parsed = Number(value);
	return Number.isInteger(parsed) && parsed > 0 ? parsed : Number.NaN;
}

function amount(value: string): number | undefined {
	if (!value.trim()) return undefined;
	const parsed = parseDanishAmount(value);
	return parsed !== null && parsed > 0 ? parsed : Number.NaN;
}

function toPayload(input: {
	date: string;
	text: string;
	documentId: string;
	bankId: string;
	lines: LineForm[];
}): AccountingDraftPayload {
	const documentId = optionalPositiveInteger(input.documentId);
	const sourceBankTransactionId = optionalPositiveInteger(input.bankId);
	if (Number.isNaN(documentId) || Number.isNaN(sourceBankTransactionId))
		throw new Error("Bilags- og bank-id skal være positive heltal.");
	const lines: AccountingDraftLine[] = input.lines.map((line, index) => {
		const debitAmount = amount(line.debitAmount);
		const creditAmount = amount(line.creditAmount);
		if (!line.accountNo.trim())
			throw new Error(`Linje ${index + 1}: angiv konto.`);
		if (
			Number.isNaN(debitAmount) ||
			Number.isNaN(creditAmount) ||
			(debitAmount === undefined) === (creditAmount === undefined)
		) {
			throw new Error(
				`Linje ${index + 1}: angiv enten et positivt debet- eller kreditbeløb.`,
			);
		}
		return {
			accountNo: line.accountNo.trim(),
			...(debitAmount === undefined ? {} : { debitAmount }),
			...(creditAmount === undefined ? {} : { creditAmount }),
			...(line.vatCode.trim() ? { vatCode: line.vatCode.trim() } : {}),
			...(line.text.trim() ? { text: line.text.trim() } : {}),
		};
	});
	return {
		transactionDate: input.date,
		text: input.text.trim(),
		...(documentId === undefined ? {} : { documentId }),
		...(sourceBankTransactionId === undefined
			? {}
			: { sourceBankTransactionId }),
		lines,
	};
}

export function AccountingDraftsView() {
	const { slug = "" } = useParams();
	const state = useAsync(
		(signal) => api.accountingDrafts(slug, { signal }),
		[slug],
	);
	const policy = useAsync(() => api.accountingApprovalPolicy(slug), [slug]);
	const [draftId, setDraftId] = useState("");
	const [date, setDate] = useState(todayIso());
	const [text, setText] = useState("");
	const [documentId, setDocumentId] = useState("");
	const [bankId, setBankId] = useState("");
	const [lines, setLines] = useState<LineForm[]>([
		{ ...EMPTY_LINE },
		{ ...EMPTY_LINE },
	]);
	const [editing, setEditing] = useState<AccountingDraft | null>(null);
	const [busy, setBusy] = useState(false);
	const [actionError, setActionError] = useState<string | null>(null);
	const [pending, setPending] = useState<{
		kind: "submit" | "reject" | "approve";
		draft: AccountingDraft;
	} | null>(null);
	const [statusFilter, setStatusFilter] = useState<
		AccountingDraft["status"] | ""
	>("");

	const outcome = useMutationOutcome(state.reload);
	useUnsavedChanges(
		Boolean(
			draftId ||
				text ||
				documentId ||
				bankId ||
				lines.some(
					(line) =>
						line.accountNo ||
						line.debitAmount ||
						line.creditAmount ||
						line.vatCode ||
						line.text,
				),
		),
	);

	async function create(event: FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		setBusy(true);
		setActionError(null);
		try {
			const payload = toPayload({ date, text, documentId, bankId, lines });
			if (editing)
				await outcome.run(() =>
					api.reviseAccountingDraft(slug, editing, payload),
				);
			else
				await outcome.run(() =>
					api.createAccountingDraft(slug, draftId, payload),
				);
			setDraftId("");
			setText("");
			setDocumentId("");
			setBankId("");
			setLines([{ ...EMPTY_LINE }, { ...EMPTY_LINE }]);
			setEditing(null);
			state.reload();
		} catch (error) {
			setActionError(
				error instanceof Error ? error.message : "Kladde kunne ikke oprettes.",
			);
		} finally {
			setBusy(false);
		}
	}

	function edit(draft: AccountingDraft) {
		setEditing(draft);
		setDraftId(draft.id);
		setDate(draft.payload.transactionDate);
		setText(draft.payload.text);
		setDocumentId(draft.payload.documentId?.toString() ?? "");
		setBankId(draft.payload.sourceBankTransactionId?.toString() ?? "");
		setLines(
			draft.payload.lines.map((line) => ({
				accountNo: line.accountNo,
				debitAmount: line.debitAmount?.toString() ?? "",
				creditAmount: line.creditAmount?.toString() ?? "",
				vatCode: line.vatCode ?? "",
				text: line.text ?? "",
			})),
		);
		window.scrollTo?.({ top: 0, behavior: "smooth" });
	}

	async function action(
		kind: "submit" | "reject" | "approve",
		draft: AccountingDraft,
		note: string,
	) {
		setActionError(null);
		try {
			if (kind === "submit") await api.submitAccountingDraft(slug, draft);
			else if (kind === "reject") {
				if (!note.trim())
					throw new Error("En afvisning kræver en begrundelse.");
				await api.rejectAccountingDraft(
					slug,
					draft,
					note,
					policy.data?.eventHash,
				);
			} else
				await api.approveAndPostAccountingDraft(
					slug,
					draft,
					policy.data?.eventHash,
				);
			state.reload();
		} catch (error) {
			setActionError(
				error instanceof ApiError || error instanceof Error
					? error.message
					: "Handlingen kunne ikke gennemføres.",
			);
			throw error;
		}
	}

	if (state.loading && !state.data)
		return <Loading label="Henter bogføringskladder…" />;
	if ((state.error && !state.data) || policy.error)
		return (
			<ErrorState
				message={state.error ?? policy.error ?? "Kladder kunne ikke hentes"}
				onRetry={() => {
					state.reload();
					policy.reload();
				}}
			/>
		);

	const drafts = state.data!.filter(
		(draft) => !statusFilter || draft.status === statusFilter,
	);

	return (
		<section
			data-cockpit-page="drafts"
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
			{outcome.feedback}
			<PageHeader
				title="Bogføringskladder"
				actions={
					<>
						<ButtonLink
							to={`/companies/${slug}/posteringer`}
							variant={"secondary"}
							xstyle={[cockpitStyles.statementBtnComposition2]}
						>
							Se bogførte posteringer
						</ButtonLink>
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
						Kladde → indsendelse → uafhængig godkendelse → atomisk bogføring
					</p>
				</div>
			</PageHeader>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Den indsendte version låses med en SHA-256-identitet. Godkenderen skal
				være en anden bruger end forfatteren og indsenderen.
			</p>
			{actionError && (
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
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{actionError}
					</p>
				</div>
			)}

			<form
				onSubmit={create}
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
					{editing ? `Ny version af ${editing.id}` : "Ny kladde"}
				</h3>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGrid,
					)}
				>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.formGridLabel,
						)}
					>
						Kladde-id
						<Input
							required
							disabled={outcome.blocked || editing !== null}
							pattern="[a-z][a-z0-9-]{0,63}"
							value={draftId}
							onChange={(event) => setDraftId(event.target.value.toLowerCase())}
							placeholder="fx bank-2026-08-001"
							xstyle={[cockpitStyles.statementInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.formGridLabel,
						)}
					>
						Dato
						<Input
							disabled={outcome.blocked}
							required
							type="date"
							value={date}
							onChange={(event) => setDate(event.target.value)}
							xstyle={[cockpitStyles.statementInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.fullWidth,
							cockpitStyles.formGridLabel,
						)}
					>
						Tekst
						<Input
							disabled={outcome.blocked}
							required
							value={text}
							onChange={(event) => setText(event.target.value)}
							xstyle={[cockpitStyles.statementInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.formGridLabel,
						)}
					>
						Bilags-id (valgfrit)
						<Input
							disabled={outcome.blocked}
							inputMode="numeric"
							value={documentId}
							onChange={(event) => setDocumentId(event.target.value)}
							xstyle={[cockpitStyles.statementInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.formGridLabel,
						)}
					>
						Bankpost-id (valgfrit)
						<Input
							disabled={outcome.blocked}
							inputMode="numeric"
							value={bankId}
							onChange={(event) => setBankId(event.target.value)}
							xstyle={[cockpitStyles.statementInputComposition]}
						/>
					</label>
				</div>
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
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableDataTh,
										cockpitStyles.statementTableScrollTableThLastChild,
									)}
								>
									Konto
								</th>
								<th
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableDataTh,
										cockpitStyles.statementTableScrollTableThLastChild,
									)}
								>
									Debet
								</th>
								<th
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableDataTh,
										cockpitStyles.statementTableScrollTableThLastChild,
									)}
								>
									Kredit
								</th>
								<th
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableDataTh,
										cockpitStyles.statementTableScrollTableThLastChild,
									)}
								>
									Momskode
								</th>
								<th
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableDataTh,
										cockpitStyles.statementTableScrollTableThLastChild,
									)}
								>
									Linjetekst
								</th>
								<th
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableDataTh,
										cockpitStyles.statementTableScrollTableThLastChild,
									)}
								/>
							</tr>
						</thead>
						<tbody
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{lines.map((line, index) => (
								<tr
									key={index}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<td
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.tableDataTd,
											cockpitStyles.statementTableScrollTableTdLastChild,
										)}
									>
										<Input
											disabled={outcome.blocked}
											aria-label={`Konto linje ${index + 1}`}
											value={line.accountNo}
											onChange={(event) =>
												setLines((current) =>
													current.map((item, i) =>
														i === index
															? { ...item, accountNo: event.target.value }
															: item,
													),
												)
											}
											xstyle={[cockpitStyles.dataInputComposition]}
										/>
									</td>
									<td
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.tableDataTd,
											cockpitStyles.statementTableScrollTableTdLastChild,
										)}
									>
										<Input
											disabled={outcome.blocked}
											aria-label={`Debet linje ${index + 1}`}
											inputMode="decimal"
											value={line.debitAmount}
											onChange={(event) =>
												setLines((current) =>
													current.map((item, i) =>
														i === index
															? {
																	...item,
																	debitAmount: event.target.value,
																	creditAmount: event.target.value
																		? ""
																		: item.creditAmount,
																}
															: item,
													),
												)
											}
											xstyle={[cockpitStyles.dataInputComposition]}
										/>
									</td>
									<td
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.tableDataTd,
											cockpitStyles.statementTableScrollTableTdLastChild,
										)}
									>
										<Input
											disabled={outcome.blocked}
											aria-label={`Kredit linje ${index + 1}`}
											inputMode="decimal"
											value={line.creditAmount}
											onChange={(event) =>
												setLines((current) =>
													current.map((item, i) =>
														i === index
															? {
																	...item,
																	creditAmount: event.target.value,
																	debitAmount: event.target.value
																		? ""
																		: item.debitAmount,
																}
															: item,
													),
												)
											}
											xstyle={[cockpitStyles.dataInputComposition]}
										/>
									</td>
									<td
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.tableDataTd,
											cockpitStyles.statementTableScrollTableTdLastChild,
										)}
									>
										<Input
											disabled={outcome.blocked}
											aria-label={`Momskode linje ${index + 1}`}
											value={line.vatCode}
											onChange={(event) =>
												setLines((current) =>
													current.map((item, i) =>
														i === index
															? { ...item, vatCode: event.target.value }
															: item,
													),
												)
											}
											xstyle={[cockpitStyles.dataInputComposition]}
										/>
									</td>
									<td
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.tableDataTd,
											cockpitStyles.statementTableScrollTableTdLastChild,
										)}
									>
										<Input
											disabled={outcome.blocked}
											aria-label={`Tekst linje ${index + 1}`}
											value={line.text}
											onChange={(event) =>
												setLines((current) =>
													current.map((item, i) =>
														i === index
															? { ...item, text: event.target.value }
															: item,
													),
												)
											}
											xstyle={[cockpitStyles.dataInputComposition]}
										/>
									</td>
									<td
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.tableDataTd,
											cockpitStyles.statementTableScrollTableTdLastChild,
										)}
									>
										<Button
											variant="secondary"
											type="button"
											disabled={lines.length <= 2}
											onClick={() =>
												setLines((current) =>
													current.filter((_, i) => i !== index),
												)
											}
											xstyle={[cockpitStyles.statementBtnComposition]}
										>
											Fjern
										</Button>
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
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
						onClick={() =>
							setLines((current) => [...current, { ...EMPTY_LINE }])
						}
						xstyle={[cockpitStyles.statementBtnComposition]}
					>
						Tilføj linje
					</Button>
					{editing && (
						<Button
							variant="secondary"
							type="button"
							onClick={() => {
								setEditing(null);
								setDraftId("");
								setText("");
								setDocumentId("");
								setBankId("");
								setLines([{ ...EMPTY_LINE }, { ...EMPTY_LINE }]);
							}}
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Annullér revision
						</Button>
					)}
					<Button
						requiredPermission="company.draft.write"
						type="submit"
						disabled={outcome.blocked || busy}
						xstyle={[cockpitStyles.statementBtnComposition]}
					>
						{busy ? "Gemmer…" : editing ? "Gem ny version" : "Opret kladde"}
					</Button>
				</div>
			</form>

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
					Kladder ({drafts.length} af {state.data!.length})
				</h3>
				<FilterBar
					activeFilters={
						statusFilter ? [`Status: ${STATUS_LABEL[statusFilter]}`] : []
					}
					onReset={() => setStatusFilter("")}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Status{" "}
						<Select
							aria-label="Filtrér kladder på status"
							value={statusFilter}
							onChange={(event) =>
								setStatusFilter(
									event.target.value as AccountingDraft["status"] | "",
								)
							}
							xstyle={[cockpitStyles.statementSelectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Alle statuser
							</option>
							{Object.entries(STATUS_LABEL).map(([value, label]) => (
								<option
									key={value}
									value={value}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{value === "submitted"
										? "Indsendt – afventer godkendelse"
										: label}
								</option>
							))}
						</Select>
					</label>
				</FilterBar>
				{drafts.length === 0 ? (
					statusFilter ? (
						<PageState kind="empty" title="Ingen kladder i visningen">
							Prøv at nulstille statusfiltret.
						</PageState>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen kladder endnu.
						</p>
					)
				) : (
					<ResponsiveTable
						label="Bogføringskladder"
						xstyle={[cockpitStyles.tableDataComposition]}
					>
						<thead
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableThead,
							)}
						>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTh,
										cockpitStyles.responsiveTableThLastChild,
										cockpitStyles.tableDataTh,
									)}
								>
									Id / version
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTh,
										cockpitStyles.responsiveTableThLastChild,
										cockpitStyles.tableDataTh,
									)}
								>
									Status
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTh,
										cockpitStyles.responsiveTableThLastChild,
										cockpitStyles.tableDataTh,
									)}
								>
									Dato og tekst
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTh,
										cockpitStyles.responsiveTableThLastChild,
										cockpitStyles.tableDataTh,
									)}
								>
									Beløb
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTh,
										cockpitStyles.responsiveTableThLastChild,
										cockpitStyles.tableDataTh,
									)}
								>
									Evidens
								</th>
								<th
									scope="col"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTh,
										cockpitStyles.responsiveTableThLastChild,
										cockpitStyles.tableDataTh,
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
								cockpitStyles.responsiveTableTbody,
							)}
						>
							{drafts.map((draft) => (
								<tr
									key={draft.id}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}
								>
									<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
										<strong
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{draft.id}
										</strong>
										<div
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											v{draft.version}
										</div>
									</td>
									<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
										<span
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{STATUS_LABEL[draft.status]}
										</span>
										{draft.reason && (
											<div
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.muted,
												)}
											>
												{draft.reason}
											</div>
										)}
									</td>
									<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
										{draft.payload.transactionDate}
										<div
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											{draft.payload.text}
										</div>
									</td>
									<td
										{...stylex.props(cockpitStyles.tableDataTdNumComposition11)}
									>
										{formatKroner(
											draft.payload.lines.reduce(
												(sum, line) => sum + (line.debitAmount ?? 0),
												0,
											),
										)}
									</td>
									<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
										<code
											title={draft.eventHash}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{draft.eventHash.slice(0, 12)}…
										</code>
										{draft.journalEntryId && (
											<div
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.muted,
												)}
											>
												Journal #{draft.journalEntryId}
											</div>
										)}
									</td>
									<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
										<div
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.rowActions,
											)}
										>
											{(draft.status === "created" ||
												draft.status === "revised") && (
												<>
													<Button
														disabled={outcome.blocked}
														requiredPermission="company.draft.write"
														type="button"
														onClick={() =>
															setPending({ kind: "submit", draft })
														}
														xstyle={[cockpitStyles.statementBtnComposition]}
													>
														Indsend
													</Button>
													<Button
														disabled={outcome.blocked}
														requiredPermission="company.draft.write"
														variant="secondary"
														type="button"
														onClick={() => edit(draft)}
														xstyle={[cockpitStyles.statementBtnComposition]}
													>
														Redigér ny version
													</Button>
												</>
											)}
											{draft.status === "rejected" && (
												<Button
													disabled={outcome.blocked}
													requiredPermission="company.draft.write"
													variant="secondary"
													type="button"
													onClick={() => edit(draft)}
													xstyle={[cockpitStyles.statementBtnComposition]}
												>
													Ret og opret ny version
												</Button>
											)}
											{draft.status === "submitted" && (
												<>
													<Button
														disabled={outcome.blocked}
														requiredPermission="company.review"
														type="button"
														onClick={() =>
															setPending({ kind: "approve", draft })
														}
														xstyle={[cockpitStyles.statementBtnComposition]}
													>
														Godkend og bogfør
													</Button>
													<Button
														disabled={outcome.blocked}
														requiredPermission="company.review"
														variant="secondary"
														type="button"
														onClick={() =>
															setPending({ kind: "reject", draft })
														}
														xstyle={[cockpitStyles.statementBtnComposition]}
													>
														Afvis
													</Button>
												</>
											)}
										</div>
									</td>
								</tr>
							))}
						</tbody>
					</ResponsiveTable>
				)}
			</section>

			{pending && (
				<ConfirmDialog
					title={
						pending.kind === "approve"
							? "Godkend og bogfør"
							: pending.kind === "reject"
								? "Afvis kladde"
								: "Indsend kladde"
					}
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{pending.kind === "approve"
								? "Den præcise indsendte version bogføres irreversibelt og audit-logges."
								: pending.kind === "reject"
									? "Kladdeversionen afvises; skriv hvorfor."
									: "Den aktuelle version låses til uafhængigt review."}
						</p>
					}
					confirmLabel={
						pending.kind === "approve"
							? "Godkend og bogfør"
							: pending.kind === "reject"
								? "Afvis"
								: "Indsend"
					}
					confirmKind={pending.kind === "reject" ? "danger" : "primary"}
					noteLabel={pending.kind === "reject" ? "Begrundelse" : undefined}
					onConfirm={(note) => action(pending.kind, pending.draft, note)}
					onClose={() => setPending(null)}
					onRefresh={state.reload}
				/>
			)}
		</section>
	);
}
