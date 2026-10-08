import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Moms — the per-company VAT return (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/vat?year=` for the registered VAT cadence —
// output VAT (salgsmoms), input VAT (købsmoms), the resulting payable amount,
// AND the full SKAT TastSelv momsangivelse rubrics (rubrik A/B/C, foreign
// goods/services VAT) so the owner can file straight from the cockpit instead
// of dropping to the terminal (#257). All money fields are kroner —
// `formatKroner` is used throughout.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { PageState, StatusChip } from "../components/CockpitPrimitives";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import {
	formatDateDa,
	formatKroner,
	tastSelvNumber,
	todayIso,
} from "../lib/format";
import type {
	CompanyVat,
	CompanyVatRegistered,
	VatRubrikker,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function VatView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const state = useAsync<CompanyVat>(
		(signal) => api.vat(slug, year, { signal }),
		[slug, year],
	);
	// True while the close-period ConfirmDialog is open (#287).
	const [closing, setClosing] = useState(false);
	// True while the reopen-period ConfirmDialog is open (#301).
	const [reopening, setReopening] = useState(false);
	// #301: the second, explicit confirmation a future-end period close needs.
	const [futureEndAcknowledged, setFutureEndAcknowledged] = useState(false);
	// Set after a successful period close / reopen — surfaced as a success banner.
	const [closedNotice, setClosedNotice] = useState<string | null>(null);

	if (state.loading && !state.data)
		return (
			<section
				data-evidence-issue="656"
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
					Moms og lukkeparathed
				</h2>
				<p
					data-evidence-status="loading"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Henter momsparathed
				</p>
				<Loading label="Henter moms…" />
			</section>
		);
	if (state.error && !state.data)
		return (
			<section
				data-evidence-issue="656"
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
					Moms og lukkeparathed
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
						? "Moms kræver afklaring"
						: "Momsparathed kunne ikke hentes"}
				</p>
				<ErrorState message={state.error} onRetry={state.reload} />
			</section>
		);

	const v = state.data!;
	const currency = v.company.currency || "DKK";

	// A non-VAT-registered company has nothing to show on the moms-tab —
	// surface an explanation card instead of an empty rubrikker table. Render
	// early so none of the period-related controls below execute against the
	// missing period fields, and TypeScript narrows v to CompanyVatRegistered
	// below.
	if (!v.vatRegistered) {
		return (
			<section
				data-cockpit-page="vat"
				data-evidence-issue="656"
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
				<PageHeader evidenceHeading title="Moms og lukkeparathed">
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
							{v.company.cvr ? `CVR ${v.company.cvr} · ` : ""}
							{v.company.country} · {currency} · Moms
						</p>
					</div>
					<p
						data-evidence-status="empty"
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen momspligt
					</p>
				</PageHeader>
				<CompanyNav
					slug={slug}
					years={v.fiscalYears}
					selectedYear={v.selectedYear}
					onYearChange={setYear}
				/>
				<Banner kind="warning">
					Denne virksomhed er ikke momsregistreret. Der er derfor ingen
					momsperiode, ingen momsangivelse og ingen SKAT-frist. Tilkøb af bilag
					med moms bogføres med{" "}
					<code
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}
					>
						expense book --vat-treatment non_deductible
					</code>{" "}
					så momsen absorberes i udgiften. Skal selskabet alligevel
					momsregistreres, så sæt en momsperiode under{" "}
					<Link
						to={`/${slug}/manage`}
						{...stylex.props(cockpitStyles.aComposition)}
					>
						Indstillinger
					</Link>
					.
				</Banner>
			</section>
		);
	}

	if (
		v.outputVat === 0 &&
		v.inputVat === 0 &&
		v.payable === 0 &&
		v.rubrikker.momsIAlt === 0
	) {
		return (
			<section
				data-cockpit-page="vat"
				data-evidence-issue="656"
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
						<h2
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h2,
								cockpitStyles.pageHeadH2,
							)}
						>
							{v.company.name}
						</h2>
						<h3
							data-evidence-heading
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h3,
							)}
						>
							Moms og lukkeparathed
						</h3>
						<p
							data-evidence-status="empty"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen momsforpligtelser i perioden
						</p>
					</div>
				</div>
				<PageState kind="empty" title="Ingen momsforpligtelser i perioden">
					Der er ingen momsbeløb at gennemgå for den valgte periode.
				</PageState>
			</section>
		);
	}

	// TypeScript narrows v to CompanyVatRegistered after the !v.vatRegistered
	// early return above — every period/deadline/rubrikker field is non-null
	// from here on. No `!` or `?? ""` shims needed.
	const payablePositive = v.payable >= 0;
	// #301: a period whose end date is still in the future has not ended yet —
	// closing it now is almost always a mistake, so the confirm dialog warns.
	const periodEndsInFuture = v.periodEnd > todayIso();
	// #301: a closed (not reported) period can be reopened from the cockpit.
	const canReopen = !v.archived && v.periodStatus === "closed";
	// #303: a momsangivelse is only filing-ready for a closed/reported period.
	const provisional = !v.archived && !v.momsangivelseReady;
	const filingStatus =
		v.periodStatus === "reported" || v.periodStatus === "closed"
			? "Lukket/endelig"
			: v.vatReportErrors.length > 0
				? "Ikke klar"
				: v.vatReportWarnings.length > 0
					? "Kræver stillingtagen"
					: "Klar";
	const statusTone =
		filingStatus === "Ikke klar"
			? "danger"
			: filingStatus === "Kræver stillingtagen"
				? "warning"
				: "success";

	return (
		<section
			data-cockpit-page="vat"
			data-evidence-issue="656"
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
				title="Moms og lukkeparathed"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							{/* #287: closing the VAT period is the prerequisite for a
              momsangivelse — hidden for an archived (read-only) year. Once the
              period is closed the action becomes a reopen instead (#301). */}
							{!v.archived &&
								v.periodStatus === "open" &&
								v.vatReportErrors.length === 0 && (
									<Button
										requiredPermission="company.review"
										type="button"
										onClick={() => {
											setFutureEndAcknowledged(false);
											setClosing(true);
										}}
										xstyle={[cockpitStyles.statementBtnComposition]}
									>
										Luk momsperiode
									</Button>
								)}
							{canReopen && (
								<Button
									requiredPermission="company.review"
									variant="secondary"
									type="button"
									onClick={() => setReopening(true)}
									xstyle={[cockpitStyles.statementBtnComposition]}
								>
									Genåbn momsperiode
								</Button>
							)}
							{/* #464 — moms-rapport som printbar PDF inkl. SKAT-rubrikker + frist. */}
							<a
								href={api.vatPdfUrl(slug, v.selectedYear)}
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
						{v.company.cvr ? `CVR ${v.company.cvr} · ` : ""}
						{v.company.country} · {currency} · Moms
					</p>
				</div>

				<p
					data-evidence-status={
						v.vatReportErrors.length || v.vatReportWarnings.length
							? "warning-or-blocked"
							: "normal"
					}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{v.vatReportErrors.length || v.vatReportWarnings.length
						? "Moms kræver afklaring"
						: "Momsparathed klar"}
				</p>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={v.fiscalYears}
				selectedYear={v.selectedYear}
				onYearChange={setYear}
			/>

			{closedNotice && <Banner kind="success">{closedNotice}</Banner>}

			<details
				data-evidence-progressive
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<summary
					data-evidence-core-action
					{...stylex.props(cockpitStyles.summaryComposition)}
				>
					Gennemgå momsparathed
				</summary>
				<p
					data-evidence-task-outcome
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Se lukkegrundlag
				</p>
			</details>

			<section
				aria-label="Indberetningsklarhed"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.statementCardHead,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}
					>
						Indberetning
					</h3>
					<StatusChip tone={statusTone}>{filingStatus}</StatusChip>
				</div>
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
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{payablePositive ? "Moms at betale" : "Moms tilgode"}:
						</strong>{" "}
						{formatKroner(v.payable, currency)}
					</span>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Frist:
						</strong>{" "}
						{formatDateDa(v.deadline)}
					</span>
				</div>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{provisional
						? "Luk og review først; en åben periode kan ikke indberettes."
						: "Tallene er endelige. Kontrollér og overfør derefter felterne i TastSelv."}
				</p>
			</section>

			{v.vatReportErrors.length > 0 && (
				<PageState kind="blocked" title="Momsrapporten kan ikke indberettes">
					Påvirkning: SKAT-felterne er ikke et sikkert indberetningsgrundlag.
					Beslutning: ret fejlene og genberegn.{" "}
					<Link
						to={`/companies/${slug}/opmaerksomhed`}
						{...stylex.props(cockpitStyles.aComposition)}
					>
						Åbn opgaver der kræver opmærksomhed
					</Link>
					. {v.vatReportErrors.join(" ")}
				</PageState>
			)}

			{v.vatReportWarnings.length > 0 && (
				<PageState kind="warning" title="Moms kræver stillingtagen">
					Påvirkning: beløbet kan være korrekt, men kræver faglig gennemgang.
					Beslutning: gennemgå advarslerne før indberetning.{" "}
					<Link
						to={`/companies/${slug}/opmaerksomhed`}
						{...stylex.props(cockpitStyles.aComposition)}
					>
						Åbn opgaver der kræver opmærksomhed
					</Link>
					. {v.vatReportWarnings.join(" ")}
				</PageState>
			)}

			{closing && (
				<ConfirmDialog
					title="Luk momsperiode"
					body={
						<>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Luk momsperioden{" "}
								<strong
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{v.periodLabel}
								</strong>{" "}
								({v.periodStart} – {v.periodEnd}). En lukket periode er en
								forudsætning for at indberette momsangivelsen, og bogføring i
								perioden låses bagefter.
							</p>
							{/* #301: a future period-end means the period is not over yet.
                  Warn clearly and require a second, explicit acknowledgement
                  before the close can go through. */}
							{periodEndsInFuture && (
								<>
									<Banner kind="warning">
										Denne momsperiode er{" "}
										<strong
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											ikke afsluttet endnu
										</strong>{" "}
										— slutdatoen {v.periodEnd} ligger i fremtiden. Lukker du nu,
										blokeres bogføring med dato i perioden, og tallene i
										momsangivelsen kan stadig nå at ændre sig. Luk normalt først
										perioden, når den er forbi.
									</Banner>
									<label
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<Input
											type="checkbox"
											checked={futureEndAcknowledged}
											onChange={(e) =>
												setFutureEndAcknowledged(e.target.checked)
											}
											xstyle={[cockpitStyles.statementInputComposition]}
										/>
										Jeg forstår at perioden ikke er afsluttet, og vil lukke den
										alligevel.
									</label>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										Lukker du ved en fejl, kan perioden genåbnes igen herfra
										(Genåbn momsperiode).
									</p>
								</>
							)}
						</>
					}
					confirmLabel="Luk perioden"
					confirmKind="danger"
					onConfirm={async () => {
						// #301: a future-end period close requires the explicit extra
						// acknowledgement. Without it the close is blocked with a clear
						// message rather than going through silently.
						if (periodEndsInFuture && !futureEndAcknowledged) {
							throw {
								code: "bad_request",
								message:
									"Bekræft først at du vil lukke en periode der ikke er afsluttet endnu — sæt flueben i feltet ovenfor.",
							};
						}
						const packet = await api.closeReadiness(
							slug,
							v.periodStart,
							v.periodEnd,
						);
						if (packet.blockers > 0) {
							throw {
								code: "bad_request",
								message:
									"Momsperioden har blokerende close-kontroller. Åbn Periodelås for at gennemgå dem.",
							};
						}
						const review = await api.reviewCloseReadiness(
							slug,
							v.periodStart,
							v.periodEnd,
						);
						if (review.packet.hash !== packet.hash) {
							throw {
								code: "conflict",
								message:
									"Grundlaget ændrede sig under review. Kontrollér momsperioden igen før lukning.",
							};
						}
						await api.closePeriod(slug, {
							periodStart: v.periodStart,
							periodEnd: v.periodEnd,
							kind: "vat_period",
							packetHash: review.packet.hash,
							reviewId: review.id,
						});
						setClosedNotice(`Momsperioden er lukket — tallene genindlæses nu.`);
						state.reload();
					}}
					onClose={() => setClosing(false)}
					onRefresh={state.reload}
				/>
			)}

			{reopening && (
				<ReopenDialog
					vat={v}
					onRefresh={state.reload}
					onReopened={(label) => {
						setClosedNotice(
							`Momsperioden ${label} er genåbnet — bogføring i perioden er tilladt igen.`,
						);
						state.reload();
					}}
					onClose={() => setReopening(false)}
				/>
			)}

			{v.archived ? (
				<ArchivedNotice year={v.selectedYear} />
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
						{v.periodLabel} · {v.periodStart} – {v.periodEnd}
					</p>

					{/* #303: for an OPEN period the figures are not final — say so,
              honestly, instead of presenting a ready-to-file momsangivelse. */}
					{provisional && (
						<Banner kind="warning">
							Åben periode — foreløbige tal. Momsperioden {v.periodLabel} er
							ikke lukket endnu, så tallene kan stadig ændre sig og udgør ikke
							en indberetningsklar momsangivelse. Luk perioden, når den er
							forbi, for at få de endelige tal.
						</Banner>
					)}
					<div
						data-evidence-data
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.card,
							cockpitStyles.statementCard,
						)}
						data-ui="statement-card"
					>
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
									cockpitStyles.statementTableScrollTable,
								)}
							>
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
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition,
											)}
										>
											Udgående moms før tab (kontrol)
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{formatKroner(v.outputVat, currency)}
										</td>
									</tr>
									{/* A bad-debt write-off (debitortab) claims back the output
                    VAT on a receivable that will never be paid. It is shown
                    on its own line so it never silently turns the salgsmoms
                    headline above negative (#271). */}
									{v.outputVatAdjustment !== 0 && (
										<tr
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
												Regulering for tab på debitorer (debitortab)
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition,
												)}
											>
												{formatKroner(v.outputVatAdjustment, currency)}
											</td>
										</tr>
									)}
									<tr
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
											Købsmoms (indgående moms)
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{formatKroner(v.inputVat, currency)}
										</td>
									</tr>
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
											{payablePositive ? "Moms at betale" : "Moms tilgode"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition9,
												payablePositive &&
													cockpitStyles.statementResultPositiveTdNum,
												!payablePositive &&
													cockpitStyles.statementResultNegativeTdNum,
											)}
										>
											{formatKroner(v.payable, currency)}
										</td>
									</tr>
								</tbody>
							</table>
						</div>
					</div>

					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.card,
							cockpitStyles.statementCard,
							cockpitStyles.vatDeadline,
						)}
						data-ui="statement-card"
					>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.vatDeadlineLabel,
								)}
							>
								Angives og betales senest
							</span>
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.vatDeadlineDate,
								)}
							>
								{formatDateDa(v.deadline)}
							</span>
						</div>
						<DeadlineCountdown days={v.daysRemaining} />
					</div>

					<RubrikkerCard
						rubrikker={v.rubrikker}
						currency={currency}
						provisional={provisional}
					/>

					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statementCheck,
							cockpitStyles.statementCheckOk,
						)}
					>
						{payablePositive
							? "Momstilsvaret nedenfor er det samlede beløb, der skal afregnes til SKAT."
							: "Momstilsvaret nedenfor er det samlede beløb, der udbetales fra SKAT."}
					</p>
				</>
			)}
		</section>
	);
}

/**
 * The full SKAT TastSelv momsangivelse rubrics — the exact numbers an owner
 * types into the momsangivelse on skat.dk. Surfacing these (#257) means the
 * cockpit's VAT view is filing-complete: rubrik A/B/C and the foreign
 * goods/services VAT no longer force a trip to the terminal's
 * `vat momsangivelse`.
 *
 * #303: for an OPEN period the terminal `vat momsangivelse` refuses to produce
 * a momsangivelse at all (it requires a closed/reported VAT period).
 * The card must therefore NOT claim its figures match that command — instead
 * it marks them provisional and says a closed period is the prerequisite.
 */
/** All rubrikker as label/raw-number pairs, in the order shown in the UI. */
function rubrikkerCsvRows(rubrikker: VatRubrikker): Array<[string, string]> {
	return [
		["Salgsmoms", tastSelvNumber(rubrikker.salgsmoms)],
		["Købsmoms", tastSelvNumber(rubrikker.kobsmoms)],
		[
			"Moms af varekøb i udlandet",
			tastSelvNumber(rubrikker.momsAfVarekobUdland),
		],
		[
			"Moms af ydelseskøb i udlandet",
			tastSelvNumber(rubrikker.momsAfYdelseskobUdland),
		],
		["Rubrik A - varer", tastSelvNumber(rubrikker.rubrikAVarer)],
		["Rubrik A - ydelser", tastSelvNumber(rubrikker.rubrikAYdelser)],
		[
			"Rubrik B - varer / EU-salg uden moms",
			tastSelvNumber(rubrikker.rubrikBVarerEuSalesList),
		],
		[
			"Rubrik B - varer / ikke EU-salg-listen",
			tastSelvNumber(rubrikker.rubrikBVarerIkkeEuSalesList),
		],
		["Rubrik B - ydelser", tastSelvNumber(rubrikker.rubrikBYdelser)],
		["Rubrik C - øvrige momsfrie salg", tastSelvNumber(rubrikker.rubrikC)],
		[
			"Olie- og flaskegasafgift",
			tastSelvNumber(rubrikker.olieOgFlaskegasafgift),
		],
		["Elafgift", tastSelvNumber(rubrikker.elafgift)],
		[
			"Naturgas- og bygasafgift",
			tastSelvNumber(rubrikker.naturgasOgBygasafgift),
		],
		["Kulafgift", tastSelvNumber(rubrikker.kulafgift)],
		["CO2-afgift", tastSelvNumber(rubrikker.co2Afgift)],
		["Vandafgift", tastSelvNumber(rubrikker.vandafgift)],
		["Moms i alt", tastSelvNumber(rubrikker.momsIAlt)],
	];
}

/**
 * One row's Kopier-button. Copies ONLY the raw TastSelv-format number (no
 * thousand separator, no "kr.") to the clipboard, then briefly shows
 * "Kopieret" without stealing focus. For a provisional (open-period) row the
 * button is disabled with a title explaining why — the owner must not paste
 * foreløbige tal into a real momsangivelse.
 */
function CopyRubrikButton({
	rawValue,
	label,
	provisional,
	onCopied,
	copied,
}: {
	rawValue: string;
	label: string;
	provisional: boolean;
	onCopied: () => void;
	copied: boolean;
}) {
	async function handleCopy() {
		try {
			await navigator.clipboard.writeText(rawValue);
			onCopied();
		} catch {
			// Clipboard write can fail (insecure context, permission denied) —
			// silently ignore: the owner can still read the figure on screen.
		}
	}
	return (
		<span
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.rubrikCopy,
			)}
		>
			{copied && (
				<span
					aria-live="polite"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rubrikCopyFeedback,
					)}
				>
					Kopieret
				</span>
			)}
			<Button
				type="button"
				onClick={handleCopy}
				disabled={provisional}
				aria-label={`Kopier ${label}`}
				title={
					provisional
						? "Periode ikke lukket — luk først for at få endelige tal"
						: `Kopier ${label} til udklipsholderen`
				}
				xstyle={[cockpitStyles.rubrikCopyBtnDisabledComposition]}
			>
				Kopier
			</Button>
		</span>
	);
}

function RubrikkerCard({
	rubrikker,
	currency,
	provisional,
}: {
	rubrikker: VatRubrikker;
	currency: string;
	provisional: boolean;
}) {
	const owedPositive = rubrikker.momsIAlt >= 0;
	// The label of the row most recently copied — drives the "Kopieret"
	// confirmation, scoped per row so two adjacent buttons don't share state.
	const [copiedLabel, setCopiedLabel] = useState<string | null>(null);
	// Cleared after a short interval so the confirmation does not linger.
	function markCopied(label: string) {
		setCopiedLabel(label);
		setTimeout(() => {
			setCopiedLabel((current) => (current === label ? null : current));
		}, 1500);
	}
	async function copyAllAsCsv() {
		const rows = rubrikkerCsvRows(rubrikker);
		const csv = rows.map(([label, value]) => `${label};${value}`).join("\n");
		try {
			await navigator.clipboard.writeText(csv);
			markCopied("__csv__");
		} catch {
			// ignore — see CopyRubrikButton.
		}
	}
	function rubrikRow(
		label: string,
		value: number,
		emphasis?: "subtotal" | "result",
	) {
		return (
			<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<td
					{...stylex.props(
						cockpitStyles.tableStatementTableTdComposition4,
						emphasis === "subtotal" && cockpitStyles.statementSubtotalTd,
						emphasis === "result" && cockpitStyles.statementResultTd,
					)}
				>
					{label}
				</td>
				<td
					{...stylex.props(
						cockpitStyles.tdNumComposition,
						emphasis === "subtotal" && cockpitStyles.statementSubtotalTd,
						emphasis === "result" && cockpitStyles.statementResultTd,
						emphasis === "result" &&
							value >= 0 &&
							cockpitStyles.statementResultPositiveTdNum,
						emphasis === "result" &&
							value < 0 &&
							cockpitStyles.statementResultNegativeTdNum,
					)}
				>
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rubrikNum,
						)}
					>
						{formatKroner(value, currency)}
					</span>
					<CopyRubrikButton
						rawValue={tastSelvNumber(value)}
						label={label}
						provisional={provisional}
						onCopied={() => markCopied(label)}
						copied={copiedLabel === label}
					/>
				</td>
			</tr>
		);
	}
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
				cockpitStyles.statementCard,
			)}
			data-ui="statement-card"
		>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.statementCardHead,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
						cockpitStyles.statementSubhead,
						cockpitStyles.statementCardHeadStatementSubhead,
					)}
				>
					{provisional
						? "SKAT-rubrikker (foreløbige — åben periode)"
						: "SKAT-rubrikker (momsangivelse)"}
				</h3>
				<Button
					type="button"
					aria-label="Kopier alle som CSV"
					onClick={copyAllAsCsv}
					disabled={provisional}
					title={
						provisional
							? "Periode ikke lukket — luk først for at få endelige tal"
							: "Kopier alle rubrikker som CSV (label;beløb) til regneark"
					}
					xstyle={[cockpitStyles.rubrikCopyCsvDisabledComposition]}
				>
					{copiedLabel === "__csv__" ? "Kopieret" : "Kopiér alle SKAT-felter"}
				</Button>
			</div>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementNote,
				)}
			>
				{provisional ? (
					<>
						De felter der hører til momsangivelsen på skat.dk (TastSelv
						Erhverv). Momsperioden er{" "}
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							ikke lukket endnu
						</strong>
						, så tallene er foreløbige og kan stadig ændre sig. Når perioden er
						forbi og lukket, bliver tallene endelige og kan indberettes.
					</>
				) : (
					<>
						Feltværdierne følger TastSelv-formens rækkefølge og hele kroner.
						Perioden er lukket, så tallene er endelige.
					</>
				)}
			</p>
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
					<tbody
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{rubrikRow("Salgsmoms", rubrikker.salgsmoms)}
						{rubrikRow("Købsmoms", rubrikker.kobsmoms)}
						{rubrikRow(
							"Moms af varekøb i udlandet (både EU og lande uden for EU)",
							rubrikker.momsAfVarekobUdland,
						)}
						{rubrikRow(
							"Moms af ydelseskøb i udlandet med omvendt betalingspligt",
							rubrikker.momsAfYdelseskobUdland,
						)}
						{rubrikRow(
							owedPositive ? "Moms i alt" : "Moms til gode i alt",
							rubrikker.momsIAlt,
							"result",
						)}
						{rubrikRow(
							"Afrundingsdifference mod rå momsrapport",
							rubrikker.wholeKronerDifferenceDkk,
						)}
					</tbody>
				</table>
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
						cockpitStyles.tableStatementTable,
					)}
				>
					<tbody
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{rubrikRow("Rubrik A — varer købt i EU", rubrikker.rubrikAVarer)}
						{rubrikRow(
							"Rubrik A — ydelser købt i EU",
							rubrikker.rubrikAYdelser,
						)}
						{rubrikRow(
							"Rubrik B — varer / EU-salg uden moms",
							rubrikker.rubrikBVarerEuSalesList,
						)}
						{rubrikRow(
							"Rubrik B — varer / ikke EU-salg-listen",
							rubrikker.rubrikBVarerIkkeEuSalesList,
						)}
						{rubrikRow("Rubrik B — ydelser", rubrikker.rubrikBYdelser)}
						{rubrikRow("Rubrik C — øvrige momsfrie salg", rubrikker.rubrikC)}
						{rubrikRow(
							"Olie- og flaskegasafgift",
							rubrikker.olieOgFlaskegasafgift,
						)}
						{rubrikRow("Elafgift", rubrikker.elafgift)}
						{rubrikRow(
							"Naturgas- og bygasafgift",
							rubrikker.naturgasOgBygasafgift,
						)}
						{rubrikRow("Kulafgift", rubrikker.kulafgift)}
						{rubrikRow("CO2-afgift", rubrikker.co2Afgift)}
						{rubrikRow("Vandafgift", rubrikker.vandafgift)}
					</tbody>
				</table>
			</div>
		</div>
	);
}

/**
 * The reopen-period confirm dialog (#301). Reopening is a controlled,
 * audit-logged action — it requires a free-text reason, recorded verbatim in
 * the audit log. The dialog reuses `ConfirmDialog`'s note field for that
 * reason and blocks the action until a reason is given.
 */
function ReopenDialog({
	vat,
	onReopened,
	onRefresh,
	onClose,
}: {
	vat: CompanyVatRegistered;
	onReopened: (label: string) => void;
	onRefresh: () => void;
	onClose: () => void;
}) {
	return (
		<ConfirmDialog
			title="Genåbn momsperiode"
			body={
				<>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Genåbn momsperioden{" "}
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{vat.periodLabel}
						</strong>{" "}
						({vat.periodStart} – {vat.periodEnd}). Bogføring med dato i perioden
						bliver tilladt igen.
					</p>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Genåbningen er en kontrolleret, fuldt revisionssporet handling — den
						tilføjes som en ny linje i revisionssporet med din begrundelse.
						Selve periode-rækken ændres aldrig. En allerede indberettet
						(reported) periode kan ikke genåbnes.
					</p>
				</>
			}
			confirmLabel="Genåbn perioden"
			confirmKind="danger"
			noteLabel="Begrundelse (påkrævet)"
			notePlaceholder="Hvorfor genåbnes perioden? (fx 'bilag bogført for sent')"
			onConfirm={async (reason) => {
				if (reason.trim().length === 0) {
					throw {
						code: "bad_request",
						message:
							"Angiv en begrundelse — en genåbning skal kunne spores i revisionssporet.",
					};
				}
				await api.reopenPeriod(vat.slug, {
					periodStart: vat.periodStart,
					periodEnd: vat.periodEnd,
					kind: "vat_period",
					reason: reason.trim(),
				});
				onReopened(vat.periodLabel);
			}}
			onClose={onClose}
			onRefresh={onRefresh}
		/>
	);
}

/**
 * The "X dage tilbage" countdown to the VAT filing deadline. Turns critical
 * once the deadline is near or passed, so an owner sees the urgency at a
 * glance — the momsangivelse is easy to forget.
 */
function DeadlineCountdown({ days }: { days: number }) {
	if (days < 0) {
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flag,
					cockpitStyles.flagCritical,
				)}
			>
				Fristen er overskredet {Math.abs(days)}{" "}
				{Math.abs(days) === 1 ? "dag" : "dage"}
			</span>
		);
	}
	if (days === 0)
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flag,
					cockpitStyles.flagCritical,
				)}
			>
				Frist i dag
			</span>
		);
	const tone = days <= 30 ? "warning" : "ok";
	return (
		<span
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.flag,
				tone === "warning" && cockpitStyles.flagWarning,
				tone === "ok" && cockpitStyles.flagOk,
			)}
		>
			{days} {days === 1 ? "dag" : "dage"} tilbage
		</span>
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
				Moms er ikke tilgængelig for {year}
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{year} er et arkiveret regnskabsår. Momsopgørelsen beregnes fra den
				aktive ledgers bogførte momskonti, og en momsangivelse kan ikke
				rekonstrueres for et arkiveret år — den vises derfor ikke.
				Resultatopgørelse, balance, saldobalance og posteringer for {year} er
				tilgængelige.
			</p>
		</div>
	);
}
