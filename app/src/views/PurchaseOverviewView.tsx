import * as stylex from "@stylexjs/stylex";
import { type FormEvent, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { PageState } from "../components/CockpitPrimitives";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Button, Input, PageHeader, Select } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type {
	PurchaseCase,
	PurchaseNeedGroup,
	PurchaseOverview,
	PurchaseSourceFact,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

const today = () => new Date().toISOString().slice(0, 10);
const yearStart = () => `${new Date().getUTCFullYear()}-01-01`;
const sourceLabel = (source: PurchaseCase["source"]) =>
	source.kind === "document"
		? `Bilag #${source.id}`
		: source.kind === "bank_transaction"
			? `Bankpost #${source.id}`
			: `Leverandørfaktura #${source.id}`;
const SourceFacts = ({
	fact,
	slug,
}: {
	fact: PurchaseSourceFact | undefined;
	slug: string;
}) => (
	<span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
		{fact?.date ?? "Dato ukendt"} · {fact?.supplier ?? "Leverandør ukendt"} ·{" "}
		{fact?.amount == null
			? "Beløb ukendt"
			: formatKroner(fact.amount, fact.currency ?? "DKK")}
		{fact?.documentId != null && (
			<>
				{" "}
				·{" "}
				<Link
					to={`/companies/${slug}/bilag?documentId=${fact.documentId}`}
					{...stylex.props(cockpitStyles.aComposition)}
				>
					Bilag #{fact.documentId}
				</Link>
			</>
		)}
	</span>
);
const documentationLabel = (outcome: PurchaseCase["documentationOutcome"]) =>
	outcome === "unresolved"
		? "Mangler review"
		: outcome === "ordinary_evidence_sufficient"
			? "Almindeligt bilag vurderet"
			: "Alternativ dokumentation vurderet";
type CaseAction = {
	purchaseCase: PurchaseCase;
	outcome: "ordinary_evidence_sufficient" | "alternative_evidence_assessed";
	reassess: boolean;
};

export function PurchaseOverviewView() {
	const { slug = "" } = useParams();
	const [searchParams] = useSearchParams();
	const requestedKind = searchParams.get("sourceKind");
	const requestedId = searchParams.get("sourceId");
	const contextKind =
		requestedKind === "document" ||
		requestedKind === "bank_transaction" ||
		requestedKind === "payable"
			? requestedKind
			: null;
	const contextId =
		requestedId &&
		Number.isInteger(Number(requestedId)) &&
		Number(requestedId) > 0
			? requestedId
			: null;
	const {
		sourceKind: initialSourceKind,
		sourceId: initialSourceId,
		fromContext: initialFromContext,
	} = contextKind && contextId
		? { sourceKind: contextKind, sourceId: contextId, fromContext: true }
		: { sourceKind: "document" as const, sourceId: "", fromContext: false };
	const [from, setFrom] = useState(yearStart);
	const [to, setTo] = useState(today);
	const [includeProvisional, setIncludeProvisional] = useState(true);
	const [selected, setSelected] = useState<PurchaseNeedGroup | null>(null);
	const [selectedMemberIds, setSelectedMemberIds] = useState<string[]>([]);
	const [caseAction, setCaseAction] = useState<CaseAction | null>(null);
	const [sourceKind, setSourceKind] = useState<
		"document" | "bank_transaction" | "payable"
	>(initialSourceKind as "document" | "bank_transaction" | "payable");
	const [sourceId, setSourceId] = useState(initialSourceId);
	const [fromContext, setFromContext] = useState(initialFromContext);
	const [createPending, setCreatePending] = useState(false);
	const [actionError, setActionError] = useState<string | null>(null);
	const state = useAsync<PurchaseOverview>(
		() => api.purchaseOverview(slug, { from, to, includeProvisional }),
		[slug, from, to, includeProvisional],
	);
	const cases = useAsync(() => api.listPurchaseCases(slug), [slug]);
	const policy = useAsync(() => api.accountingApprovalPolicy(slug), [slug]);
	if (state.loading && !state.data)
		return <PageState kind="loading" title="Henter købsoversigt" />;
	if (state.error || policy.error)
		return (
			<PageState
				kind="error"
				title="Købsoverblik kunne ikke hentes"
				onRetry={() => {
					state.reload();
					policy.reload();
				}}
			>
				{state.error ?? policy.error}
			</PageState>
		);
	const overview = state.data!;
	const reload = () => {
		state.reload();
		cases.reload();
	};
	function create(event: FormEvent) {
		event.preventDefault();
		if (!Number.isInteger(Number(sourceId)) || Number(sourceId) <= 0) {
			setActionError("Kilde-id skal være et positivt heltal.");
			return;
		}
		setCreatePending(true);
	}
	async function confirmCreate() {
		try {
			await api.createPurchaseCase(slug, {
				source: { kind: sourceKind, id: Number(sourceId) },
				documentationOutcome: "unresolved",
				idempotencyKey: crypto.randomUUID(),
			});
			setSourceId("");
			setCreatePending(false);
			reload();
		} catch (cause) {
			setActionError(
				cause instanceof Error
					? cause.message
					: "Købscasen kunne ikke oprettes.",
			);
			throw cause;
		}
	}
	async function confirmCaseAction(note: string) {
		if (!caseAction) return;
		if (caseAction.reassess && !note.trim())
			throw new Error("En begrundelse for genvurderingen er påkrævet.");
		const fresh = await api.getPurchaseCase(
			slug,
			caseAction.purchaseCase.caseId,
		);
		if (!fresh)
			throw new Error("Casen findes ikke længere. Opdatér oversigten.");
		if (caseAction.reassess) {
			if (!fresh.sourceStatus?.currentSourceFingerprint)
				throw new Error("Kilden mangler og kan ikke genvurderes.");
			await api.reassessPurchaseCase(slug, fresh.caseId, {
				expectedVersion: fresh.version,
				expectedSourceFingerprint: fresh.sourceFingerprint,
				currentSourceFingerprint: fresh.sourceStatus.currentSourceFingerprint,
				expectedPolicyEventHash: policy.data?.eventHash,
				documentationOutcome: caseAction.outcome,
				reason: note,
				idempotencyKey: crypto.randomUUID(),
			});
		} else
			await api.reviewPurchaseCase(slug, fresh.caseId, {
				expectedVersion: fresh.version,
				expectedSourceFingerprint: fresh.sourceFingerprint,
				expectedPolicyEventHash: policy.data?.eventHash,
				documentationOutcome: caseAction.outcome,
				note,
				idempotencyKey: crypto.randomUUID(),
			});
		setCaseAction(null);
		reload();
	}
	return (
		<section
			data-cockpit-page="purchase-overview"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Købsoverblik"
				description="Kildebaseret status og dokumenterede behov. Foreløbige beløb ændrer aldrig hovedbog eller momsindberetning."
			/>
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
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.rowActions,
				)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Fra{" "}
					<Input
						type="date"
						value={from}
						onChange={(event) => setFrom(event.target.value)}
						xstyle={[cockpitStyles.rowActionsInputComposition2]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Til{" "}
					<Input
						type="date"
						value={to}
						onChange={(event) => setTo(event.target.value)}
						xstyle={[cockpitStyles.rowActionsInputComposition2]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<Input
						type="checkbox"
						checked={includeProvisional}
						onChange={(event) => setIncludeProvisional(event.target.checked)}
						xstyle={[cockpitStyles.rowActionsInputComposition2]}
					/>{" "}
					Vis foreløbig effekt
				</label>
			</div>
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{formatKroner(overview.basis.canonical.economicEffect.expense)}
					</strong>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bogført omkostning
					</span>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Fra hovedbogen i perioden.
					</p>
				</article>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{includeProvisional
							? formatKroner(overview.basis.provisional.economicEffect.expense)
							: "Slået fra"}
					</strong>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Foreløbig omkostning
					</span>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Kun aktive, klassificerede drafts; ikke bogført.
					</p>
				</article>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{includeProvisional
							? formatKroner(
									overview.basis.provisional.economicEffect.expectedVat,
								)
							: "Slået fra"}
					</strong>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Forventet moms
					</span>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ikke klar til momsindberetning.
					</p>
				</article>
			</div>
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
					Åbn foreløbig købscase
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Knytter én eksisterende kilde til en arbejds-case. Den bogfører ikke
					og ændrer ikke momsstatus.
				</p>
				{fromContext ? (
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rowActions,
						)}
					>
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Valgt kilde:{" "}
							{sourceLabel({ kind: sourceKind, id: Number(sourceId) })}
						</strong>
						<Button
							type="button"
							onClick={() => setFromContext(false)}
							variant={"secondary"}
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Vælg anden kilde
						</Button>
						<Button
							requiredPermission="company.draft.write"
							type="submit"
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Åbn case
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
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Kildetype{" "}
							<Select
								value={sourceKind}
								onChange={(event) =>
									setSourceKind(event.target.value as typeof sourceKind)
								}
								xstyle={[cockpitStyles.rowActionsSelectComposition]}
							>
								<option
									value="document"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Bilag
								</option>
								<option
									value="bank_transaction"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Bankpost
								</option>
								<option
									value="payable"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Leverandørfaktura
								</option>
							</Select>
						</label>
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Kilde-id{" "}
							<Input
								required
								inputMode="numeric"
								value={sourceId}
								onChange={(event) => setSourceId(event.target.value)}
								xstyle={[cockpitStyles.rowActionsInputComposition2]}
							/>
						</label>
						<Button
							requiredPermission="company.draft.write"
							type="submit"
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Åbn case
						</Button>
					</div>
				)}
			</form>
			<div
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
					Åbne købscases
				</h3>
				{cases.loading && !cases.data ? (
					<PageState kind="loading" title="Henter købscases" />
				) : cases.error ? (
					<PageState
						kind="error"
						title="Købscases kunne ikke hentes"
						onRetry={cases.reload}
					>
						{cases.error}
					</PageState>
				) : !cases.data?.length ? (
					<PageState kind="empty" title="Ingen købscases endnu">
						Åbn en foreløbig case fra en eksisterende kilde.
					</PageState>
				) : (
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{cases.data.map((purchaseCase) => {
							const stale = purchaseCase.sourceStatus?.status !== "current";
							return (
								<li
									key={purchaseCase.caseId}
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
										{sourceLabel(purchaseCase.source)}
									</strong>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										<SourceFacts fact={purchaseCase.sourceFact} slug={slug} />
									</p>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{documentationLabel(purchaseCase.documentationOutcome)} ·{" "}
										{purchaseCase.accountingProgress === "posted"
											? "Bogført"
											: "Ikke bogført"}{" "}
										· kilde: {purchaseCase.sourceStatus?.status ?? "ukendt"} ·
										moms: {purchaseCase.vatEvidence.status}
									</p>
									{purchaseCase.accountingProgress === "unposted" && !stale && (
										<span
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.rowActions,
											)}
										>
											<Button
												requiredPermission="company.review"
												type="button"
												onClick={() =>
													setCaseAction({
														purchaseCase,
														outcome: "ordinary_evidence_sufficient",
														reassess: false,
													})
												}
												variant={"secondary"}
												xstyle={[cockpitStyles.statementBtnComposition]}
											>
												Review almindeligt bilag
											</Button>
											<Button
												requiredPermission="company.review"
												type="button"
												onClick={() =>
													setCaseAction({
														purchaseCase,
														outcome: "alternative_evidence_assessed",
														reassess: false,
													})
												}
												variant={"secondary"}
												xstyle={[cockpitStyles.statementBtnComposition]}
											>
												Review alternativ dokumentation
											</Button>
										</span>
									)}
									{purchaseCase.accountingProgress === "unposted" &&
										purchaseCase.sourceStatus?.status === "stale" && (
											<Button
												requiredPermission="company.review"
												type="button"
												onClick={() =>
													setCaseAction({
														purchaseCase,
														outcome: "ordinary_evidence_sufficient",
														reassess: true,
													})
												}
												variant={"secondary"}
												xstyle={[cockpitStyles.statementBtnComposition]}
											>
												Genvurdér ændret kilde
											</Button>
										)}
									{purchaseCase.sourceStatus?.status === "missing" && (
										<p
											role="alert"
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Kilden mangler; den kan ikke reviewes eller genvurderes.
										</p>
									)}
								</li>
							);
						})}
					</ul>
				)}
			</div>
			<div
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
					Grupperede behov
				</h3>
				{overview.groups.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen åbne grupper i perioden.
					</p>
				) : (
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{overview.groups.map((group) => {
							const selectedIds =
								selected?.selectionHash === group.selectionHash
									? selectedMemberIds
									: group.members.map((member) => member.caseId);
							const selectedCount = selectedIds.length;
							return (
								<li
									key={group.selectionHash}
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
										{group.need.question}
									</strong>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{group.caseCount} eksakte cases · {group.need.key}
									</p>
									<ul
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{group.members.map((member) => (
											<li
												key={member.caseId}
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.muted,
												)}
											>
												{group.need.key === "documentation:unresolved" && (
													<label
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}
													>
														<Input
															type="checkbox"
															checked={selectedIds.includes(member.caseId)}
															onChange={(event) => {
																if (
																	selected?.selectionHash !==
																	group.selectionHash
																) {
																	setSelected(group);
																	setSelectedMemberIds(
																		event.target.checked
																			? group.members.map((item) => item.caseId)
																			: group.members
																					.filter(
																						(item) =>
																							item.caseId !== member.caseId,
																					)
																					.map((item) => item.caseId),
																	);
																} else
																	setSelectedMemberIds((ids) =>
																		event.target.checked
																			? [...ids, member.caseId]
																			: ids.filter(
																					(id) => id !== member.caseId,
																				),
																	);
															}}
															xstyle={[cockpitStyles.statementInputComposition]}
														/>{" "}
														Vælg
													</label>
												)}{" "}
												{sourceLabel(member.source)} ·{" "}
												<SourceFacts fact={member.sourceFact} slug={slug} />
												kilde: {member.sourceStatus.status} ·{" "}
												{member.need?.key ?? "intet åbent behov"}
											</li>
										))}
									</ul>
									{group.need.key === "documentation:unresolved" && (
										<>
											<Button
												requiredPermission="company.review"
												type="button"
												disabled={selectedCount === 0}
												onClick={() => {
													setSelected(group);
													if (selected?.selectionHash !== group.selectionHash)
														setSelectedMemberIds(
															group.members.map((member) => member.caseId),
														);
												}}
												variant={"secondary"}
												xstyle={[cockpitStyles.statementBtnComposition]}
											>
												Review {selectedCount} valgt
												{selectedCount === 1 ? "" : "e"}
											</Button>
											{selected?.selectionHash === group.selectionHash &&
												selectedCount === 0 && (
													<p
														role="alert"
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}
													>
														Vælg mindst én case før review.
													</p>
												)}
										</>
									)}
								</li>
							);
						})}
					</ul>
				)}
			</div>
			{createPending && (
				<ConfirmDialog
					title="Åbn foreløbig købscase"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Casen bindes til den valgte kilde. Den bogfører ikke og godkender
							ikke moms.
						</p>
					}
					confirmLabel="Åbn case"
					onConfirm={confirmCreate}
					onClose={() => setCreatePending(false)}
				/>
			)}
			{caseAction && (
				<ConfirmDialog
					title={
						caseAction.reassess
							? "Genvurdér ændret købskilde"
							: "Review købscase"
					}
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{caseAction.reassess
								? "Du bekræfter en ny vurdering af den ændrede kilde. Den tidligere vurdering bevares i revisionssporet."
								: "Reviewet gælder den viste case og bogfører ikke eller ændrer momsstatus."}
						</p>
					}
					noteLabel={
						caseAction.reassess
							? "Begrundelse for genvurdering"
							: "Review-note (valgfri)"
					}
					notePlaceholder={
						caseAction.reassess
							? "Hvad er ændret i kilden?"
							: "Notér grundlaget for reviewet"
					}
					confirmLabel={
						caseAction.reassess ? "Bekræft genvurdering" : "Bekræft review"
					}
					onConfirm={confirmCaseAction}
					onClose={() => setCaseAction(null)}
				/>
			)}
			{selected && (
				<ConfirmDialog
					title="Review købsdokumentation"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Review kun de {selectedMemberIds.length} præcist valgte cases.
							Handlingen bogfører ikke og ændrer ikke momsstatus.
						</p>
					}
					confirmLabel="Bekræft review"
					confirmKind="primary"
					onConfirm={async (note) => {
						const members = selected.members.filter((member) =>
							selectedMemberIds.includes(member.caseId),
						);
						if (members.length === 0)
							throw new Error("Vælg mindst én case før review.");
						await api.reviewPurchaseCaseGroup(slug, {
							groupId: `ui-group-${crypto.randomUUID()}`,
							members: members.map((member) => ({
								caseId: member.caseId,
								expectedVersion: member.version,
								expectedSourceFingerprint: member.sourceFingerprint,
							})),
							expectedPolicyEventHash: policy.data?.eventHash,
							documentationOutcome: "ordinary_evidence_sufficient",
							note,
							idempotencyKey: crypto.randomUUID(),
						});
						setSelected(null);
						setSelectedMemberIds([]);
						reload();
					}}
					onClose={() => {
						setSelected(null);
						setSelectedMemberIds([]);
					}}
				/>
			)}
		</section>
	);
}
