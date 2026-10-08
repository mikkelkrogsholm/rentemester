import * as stylex from "@stylexjs/stylex";
import {
	Button,
	ButtonLink,
	Dialog,
	Input,
	PageHeader,
	Select,
	Textarea,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useCapabilities } from "../lib/useCapabilities";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
// Periodelås view (#342) — per-virksomhed regnskabsperioder med
// effective status (åben/lukket/indberettet), 'Luk periode'-knap +
// 'Genåbn periode'-knap. Cockpittet er en tynd skal over de
// eksisterende CLI-ækvivalente POST .../periods/close og /reopen
// endpoints — ingen ny core-logik introduceres.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { ApiError, api } from "../lib/api";
import { todayIso } from "../lib/format";
import type {
	AccountingPeriodKind,
	AccountingPeriodRow,
	CompanyPeriods,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

const KIND_LABEL: Record<AccountingPeriodKind, string> = {
	vat_period: "Momsperiode",
	vat_quarter: "Momsperiode",
	fiscal_year: "Regnskabsår",
	custom: "Andet",
};

const STATUS_LABEL: Record<string, string> = {
	open: "Åben",
	closed: "Lukket",
	reported: "Indberettet",
};

export function PeriodsView() {
	const { slug = "" } = useParams();
	const [error, setError] = useState<string | null>(null);
	const [openClose, setOpenClose] = useState(false);
	const [reopenTarget, setReopenTarget] = useState<AccountingPeriodRow | null>(
		null,
	);

	const state = useAsync<CompanyPeriods>(
		(signal) => api.periods(slug, { signal }),
		[slug],
	);

	const doneRefresh = state.reload;

	if (state.loading && !state.data) return <Loading />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;
	const data = state.data!;

	return (
		<section
			data-cockpit-page="period-lock"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
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
				title="Perioder og låsning"
				actions={
					<>
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
								onClick={() => {
									setError(null);
									setOpenClose(true);
								}}
								xstyle={[cockpitStyles.periodsViewBtnComposition]}
							>
								Luk periode …
							</Button>
							<ButtonLink
								to={`/companies/${slug}/manage`}
								variant={"secondary"}
								xstyle={[cockpitStyles.periodsViewBtnComposition2]}
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
						{data.company.country} · Periodelås
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
				En lukket periode kan ikke modtage nye posteringer. En indberettet
				periode (sendt til SKAT / Erhvervsstyrelsen) kan ikke genåbnes.
				Genåbning af en lukket periode appendes til audit-log'en med en
				begrundelse.
			</p>

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
					Sammentælling
				</h3>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.filterBar,
					)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Åbne: {data.byStatus.open}
					</span>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Lukkede: {data.byStatus.closed}
					</span>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Indberettede: {data.byStatus.reported}
					</span>
				</div>
			</section>

			{error && (
				<div
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</div>
			)}

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
					Perioder ({data.periods.length})
				</h3>
				{data.periods.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen lukkede perioder endnu. Lukninger fra CLI eller fra denne side
						vises her sammen med deres effective status.
					</p>
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
							aria-label="Regnskabsperioder"
							{...stylex.props(cockpitStyles.periodsViewTableTableComposition)}
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
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
										)}
									>
										Start
									</th>
									<th
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
										)}
									>
										Slut
									</th>
									<th
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
										)}
									>
										Type
									</th>
									<th
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
										)}
									>
										Status
									</th>
									<th
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
										)}
									>
										Lukket af
									</th>
									<th
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
										)}
									>
										Reference
									</th>
									<th
										{...stylex.props(
											cockpitStyles.periodsViewTableTableThComposition,
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
									cockpitStyles.responsiveTableTbody,
								)}
							>
								{data.periods.map((p) => (
									<tr
										key={p.id}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTr,
										)}
									>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition,
											)}
										>
											{p.periodStart}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition,
											)}
										>
											{p.periodEnd}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition2,
											)}
										>
											{KIND_LABEL[p.kind] ?? p.kind}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition2,
											)}
										>
											<span
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{STATUS_LABEL[p.effectiveStatus] ?? p.effectiveStatus}
											</span>
										</td>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition3,
											)}
										>
											{p.closedBy ?? "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition3,
											)}
										>
											{p.reference ?? "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.periodsViewTableTableTdComposition2,
											)}
										>
											{p.effectiveStatus === "closed" && (
												<Button
													requiredPermission="company.review"
													variant="secondary"
													type="button"
													onClick={() => {
														setError(null);
														setReopenTarget(p);
													}}
													xstyle={[cockpitStyles.periodsViewBtnComposition]}
												>
													Genåbn …
												</Button>
											)}
											{p.effectiveStatus === "reported" && (
												<span
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.muted,
													)}
												>
													Indberettet — kan ikke genåbnes
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

			{openClose && (
				<ClosePeriodModal
					slug={slug}
					onClose={() => setOpenClose(false)}
					onRefresh={state.reload}
					onDone={() => {
						setOpenClose(false);
						doneRefresh();
					}}
					onError={(msg) => setError(msg)}
				/>
			)}
			{reopenTarget && (
				<ReopenPeriodModal
					slug={slug}
					target={reopenTarget}
					onClose={() => setReopenTarget(null)}
					onRefresh={state.reload}
					onDone={() => {
						setReopenTarget(null);
						doneRefresh();
					}}
					onError={(msg) => setError(msg)}
				/>
			)}
		</section>
	);
}

function ClosePeriodModal({
	slug,
	onClose: onDismiss,
	onRefresh,
	onDone,
	onError,
}: {
	slug: string;
	onClose: () => void;
	onRefresh: () => void;
	onDone: () => void;
	onError: (msg: string) => void;
}) {
	const { can } = useCapabilities();
	const [periodStart, setPeriodStart] = useState("");
	const [periodEnd, setPeriodEnd] = useState("");
	const [kind, setKind] = useState<AccountingPeriodKind>("vat_period");
	const [reference, setReference] = useState("");
	const [submitting, setSubmitting] = useState(false);
	const [packet, setPacket] = useState<{
		hash: string;
		blockers: number;
		warnings: number;
		items: Array<{
			code: string;
			status: "passed" | "warning" | "blocked" | "unavailable";
			waivable: boolean;
			count: number;
		}>;
	} | null>(null);
	const [force, setForce] = useState(false);
	const [forceReason, setForceReason] = useState("");
	// #301 — a period whose end lies in the future is not over yet. Require a
	// second, explicit acknowledgement before such a close can go through, the
	// same guard VatView's close-modal has.
	const [futureEndAcknowledged, setFutureEndAcknowledged] = useState(false);
	const periodEndsInFuture = periodEnd !== "" && periodEnd > todayIso();

	const outcome = useMutationOutcome(onRefresh);
	const guard = useDiscardGuard(
		Boolean(
			periodStart ||
				periodEnd ||
				reference ||
				packet ||
				force ||
				forceReason ||
				futureEndAcknowledged,
		) || kind !== "vat_period",
		onDismiss,
	);
	const { onClose } = guard;

	const submit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (outcome.isBlocked()) return;
		if (periodEndsInFuture && !futureEndAcknowledged) {
			onError(
				"Bekræft først at du vil lukke en periode der ikke er afsluttet endnu — sæt flueben i feltet nedenfor.",
			);
			return;
		}
		setSubmitting(true);
		try {
			if (!packet) {
				setPacket(await api.closeReadiness(slug, periodStart, periodEnd));
				setSubmitting(false);
				return;
			}
			const review = await outcome.run(() =>
				api.reviewCloseReadiness(slug, periodStart, periodEnd),
			);
			if (review.packet.hash !== packet.hash) {
				setPacket(review.packet);
				onError(
					"Grundlaget ændrede sig. Kontrollér den nye packet før lukning.",
				);
				setSubmitting(false);
				return;
			}
			await outcome.run(() =>
				api.closePeriod(slug, {
					periodStart,
					periodEnd,
					kind,
					...(reference ? { reference } : {}),
					packetHash: review.packet.hash,
					reviewId: review.id,
					...(force ? { force: true, reason: forceReason } : {}),
				}),
			);
			guard.dismiss();
			onDone();
		} catch (err) {
			onError(
				err instanceof ApiError ? err.message : "Periodelukning fejlede.",
			);
			setSubmitting(false);
		}
	};

	return (
		<Dialog
			title="Luk periode"
			onClose={onClose}
			busy={submitting}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}

			<form
				onSubmit={submit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Start (YYYY-MM-DD)
					<Input
						disabled={outcome.blocked}
						type="date"
						value={periodStart}
						onChange={(e) => setPeriodStart(e.target.value)}
						required
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				{packet && (
					<>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Kontrolleret: {packet.blockers} blokeringer, {packet.warnings}{" "}
							advarsler. Gennemgå resultatet og vælg derefter “Gem review og
							luk”.
						</p>
						{packet.blockers > 0 && (
							<>
								<div
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Blokeringer:{" "}
									{packet.items
										.filter(
											(item) =>
												item.status === "blocked" ||
												item.status === "unavailable",
										)
										.map((item) => item.code)
										.join(", ")}
								</div>
								{can("company.period.force-close") && (
									<label
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<Input
											disabled={outcome.blocked}
											type="checkbox"
											checked={force}
											onChange={(e) => setForce(e.target.checked)}
											xstyle={[cockpitStyles.inputComposition]}
										/>{" "}
										Anmod om force-lukning af alene fravigelige blokeringer
									</label>
								)}
								{force && (
									<label
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Begrundelse for force-lukning
										<Textarea
											disabled={outcome.blocked}
											value={forceReason}
											onChange={(e) => setForceReason(e.target.value)}
											required
											rows={2}
											xstyle={[cockpitStyles.textareaComposition]}
										/>
									</label>
								)}
							</>
						)}
					</>
				)}
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Slut (YYYY-MM-DD)
					<Input
						disabled={outcome.blocked}
						type="date"
						value={periodEnd}
						onChange={(e) => setPeriodEnd(e.target.value)}
						required
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Type
					<Select
						disabled={outcome.blocked}
						value={kind}
						onChange={(e) => setKind(e.target.value as AccountingPeriodKind)}
						xstyle={[cockpitStyles.selectComposition]}
					>
						<option
							value="vat_period"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Momsperiode
						</option>
						<option
							value="fiscal_year"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Regnskabsår
						</option>
						<option
							value="custom"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Andet
						</option>
					</Select>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Reference (valgfri)
					<Input
						disabled={outcome.blocked}
						type="text"
						value={reference}
						onChange={(e) => setReference(e.target.value)}
						placeholder="fx Q1 2026 momsangivelse"
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				{/* #301: a future period-end means the period is not over yet. Warn
              clearly and require a second, explicit acknowledgement before the
              close can go through. */}
				{periodEndsInFuture && (
					<>
						<div
							role="alert"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Perioden er{" "}
							<strong
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								ikke afsluttet endnu
							</strong>{" "}
							— slutdatoen {periodEnd} ligger i fremtiden. Lukker du nu,
							blokeres bogføring med dato i perioden. Luk normalt først
							perioden, når den er forbi. En periode lukket ved en fejl kan
							genåbnes herfra.
						</div>
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<Input
								disabled={outcome.blocked}
								type="checkbox"
								checked={futureEndAcknowledged}
								onChange={(e) => setFutureEndAcknowledged(e.target.checked)}
								xstyle={[cockpitStyles.inputComposition]}
							/>
							Jeg forstår at perioden ikke er afsluttet, og vil lukke den
							alligevel.
						</label>
					</>
				)}
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<Button
						requiredPermission={
							force ? "company.period.force-close" : "company.review"
						}
						type="submit"
						disabled={
							outcome.blocked ||
							submitting || (periodEndsInFuture && !futureEndAcknowledged) ||
							(packet?.blockers !== 0 && (!force || !forceReason.trim()))
						}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{submitting
							? "Arbejder …"
							: packet
								? "Gem review og luk"
								: "Kontrollér"}
					</Button>
					<Button
						variant="secondary"
						type="button"
						onClick={onClose}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Annullér
					</Button>
				</div>
			</form>
		</Dialog>
	);
}

function ReopenPeriodModal({
	slug,
	target,
	onClose: onDismiss,
	onRefresh,
	onDone,
	onError,
}: {
	slug: string;
	target: AccountingPeriodRow;
	onClose: () => void;
	onRefresh: () => void;
	onDone: () => void;
	onError: (msg: string) => void;
}) {
	const [reason, setReason] = useState("");
	const [submitting, setSubmitting] = useState(false);

	const outcome = useMutationOutcome(onRefresh);
	const guard = useDiscardGuard(Boolean(reason), onDismiss);
	const { onClose } = guard;

	const submit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (outcome.isBlocked()) return;
		setSubmitting(true);
		try {
			await outcome.run(() =>
				api.reopenPeriod(slug, {
					periodStart: target.periodStart,
					periodEnd: target.periodEnd,
					kind: target.kind,
					reason,
				}),
			);
			guard.dismiss();
			onDone();
		} catch (err) {
			onError(err instanceof ApiError ? err.message : "Genåbning fejlede.");
			setSubmitting(false);
		}
	};

	return (
		<Dialog
			title="Genåbn periode"
			onClose={onClose}
			busy={submitting}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{target.periodStart} – {target.periodEnd} ({KIND_LABEL[target.kind]}) .
				Din begrundelse gemmes ordret i revisionssporet.
			</p>
			<form
				onSubmit={submit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Begrundelse (påkrævet)
					<Textarea
						disabled={outcome.blocked}
						value={reason}
						onChange={(e) => setReason(e.target.value)}
						rows={3}
						required
						placeholder="fx 'Bilag indlæst for sent — postering skal korrigeres'"
						xstyle={[cockpitStyles.textareaComposition]}
					/>
				</label>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<Button
						requiredPermission="company.review"
						type="submit"
						disabled={outcome.blocked || submitting || !reason.trim()}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{submitting ? "Genåbner …" : "Genåbn periode"}
					</Button>
					<Button
						variant="secondary"
						type="button"
						onClick={onClose}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Annullér
					</Button>
				</div>
			</form>
		</Dialog>
	);
}
