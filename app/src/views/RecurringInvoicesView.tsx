import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
// Faktura-skabeloner — the cockpit surface for recurring-invoice templates.
//
// The deterministic core (createRecurringInvoiceTemplate / generateRecurringInvoice
// / retireRecurringInvoiceTemplate) is already in place — this view lists the
// templates, surfaces their next-issue date, lets a human generate the next
// invoice with one click, lets the owner retire a template that should no
// longer suggest itself (#435), and — as of #386 — lets the owner create a
// new template from the cockpit instead of having to use the CLI. Generation
// is idempotent, so re-clicking is safe.
//
// Templates are append-only by schema: a retired template cannot be
// reactivated, and identity/payload columns cannot be mutated. When an owner
// needs to change terms (price, frequency, customer), they retire the old
// template and create a new one — past generations stay on the original
// template's history.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { RecurringInvoiceTemplateModal } from "../components/RecurringInvoiceTemplateModal";
import { api } from "../lib/api";
import type {
	CompanyRecurringInvoices,
	FiscalYearEntry,
	RecurringInvoiceTemplateRow,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

type Page = {
	recurringInvoices: CompanyRecurringInvoices;
	fiscalYears: FiscalYearEntry[];
};

const INTERVAL_LABELS: Record<RecurringInvoiceTemplateRow["interval"], string> =
	{
		weekly: "ugentligt",
		monthly: "månedligt",
		quarterly: "kvartalsvist",
		yearly: "årligt",
	};
const CHANNEL_LABELS: Record<"manual" | "email" | "digisense", string> = {
	manual: "manuel kladde",
	email: "e-mail",
	digisense: "e-faktura",
};

export function RecurringInvoicesView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	// #386: the create-template modal is rendered into the view; it is toggled
	// by both the page-head primary button and the empty-state CTA.
	const [createOpen, setCreateOpen] = useState(false);
	const state = useAsync<Page>(
		async (signal) => {
			const [recurringInvoices, fiscalYears] = await Promise.all([
				api.recurringInvoices(slug, { signal }),
				api.fiscalYears(slug, { signal }),
			]);
			return { recurringInvoices, fiscalYears };
		},
		[slug],
	);

	if (state.loading && !state.data)
		return <Loading label="Henter skabeloner…" />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const { recurringInvoices: r, fiscalYears } = state.data!;
	const selectedYear =
		year ??
		fiscalYears.find((y) => y.source === "live")?.label ??
		fiscalYears[0]?.label ??
		String(new Date().getFullYear());
	const active = r.templates.filter((t) => t.active);
	const retired = r.templates.filter((t) => !t.active);
	// #386: the selected fiscal year decides whether the create button is
	// shown. Archived years are read-only across the cockpit (mirrors
	// InvoicesView, BankView etc.), so an archived year hides the CTA without
	// removing the read-only listing of past templates.
	const selectedYearArchived =
		fiscalYears.find((y) => y.label === selectedYear)?.source === "archive";

	return (
		<section
			data-cockpit-page="invoice-templates"
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
				title="Faktura-skabeloner"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
								viewStyles.site0,
							)}
						>
							{!selectedYearArchived && (
								<Button
									requiredPermission="company.draft.write"
									type="button"
									onClick={() => setCreateOpen(true)}
									xstyle={[cockpitStyles.statementBtnComposition]}
								>
									Opret skabelon
								</Button>
							)}
							<ButtonLink
								to={`/companies/${slug}/fakturaer`}
								variant={"secondary"}
								xstyle={[cockpitStyles.statementBtnComposition2]}
							>
								Tilbage til fakturaer
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
						Gentagne fakturaer — den næste i hver række kan udstedes med ét
						klik. Generering er idempotent: et nyt klik på samme periode
						udsteder ikke en ny faktura.
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={fiscalYears}
				selectedYear={selectedYear}
				onYearChange={setYear}
			/>

			{r.templates.length === 0 ? (
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
						Ingen skabeloner endnu
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Der er ikke oprettet nogen faktura-skabeloner for denne virksomhed.
						Når du har en gentagen faktura — fx et månedligt abonnement eller en
						kvartalsvis ydelse — opretter du en skabelon, og cockpittet udsteder
						den næste faktura med ét klik.
					</p>
					{selectedYearArchived ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Regnskabsåret er arkiveret. Skift til et aktivt år for at oprette
							en skabelon.
						</p>
					) : (
						<Button
							requiredPermission="company.draft.write"
							type="button"
							onClick={() => setCreateOpen(true)}
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Opret skabelon
						</Button>
					)}
				</div>
			) : (
				<>
					{active.length > 0 && (
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
								Aktive ({active.length})
							</h3>
							{active.map((t) => (
								<TemplateCard
									key={t.id}
									template={t}
									slug={slug}
									onReload={state.reload}
								/>
							))}
						</div>
					)}
					{retired.length > 0 && (
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
								Tilbagetrukne ({retired.length})
							</h3>
							{retired.map((t) => (
								<TemplateCard
									key={t.id}
									template={t}
									slug={slug}
									onReload={state.reload}
								/>
							))}
						</div>
					)}
				</>
			)}

			{createOpen && (
				<RecurringInvoiceTemplateModal
					slug={slug}
					onCreated={state.reload}
					onClose={() => setCreateOpen(false)}
				/>
			)}
		</section>
	);
}

/** One template's card — header, generate action, retire action, and history. */
function TemplateCard({
	template,
	slug,
	onReload,
}: {
	template: RecurringInvoiceTemplateRow;
	slug: string;
	onReload: () => void;
}) {
	const [asOfDate, setAsOfDate] = useState(template.nextIssueDate);
	const [savedAsOfDate, setSavedAsOfDate] = useState(template.nextIssueDate);
	const [busy, setBusy] = useState(false);
	const [retireBusy, setRetireBusy] = useState(false);
	const [retiring, setRetiring] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [notice, setNotice] = useState<string | null>(null);

	const outcome = useMutationOutcome(onReload);
	const markDateSaved = useUnsavedChanges(asOfDate !== savedAsOfDate);
	async function generate() {
		if (outcome.isBlocked()) return;
		setBusy(true);
		setError(null);
		setNotice(null);
		try {
			const result = await outcome.run(() =>
				api.generateRecurringInvoice(slug, template.id, asOfDate),
			);
			if (result.created) {
				setNotice(
					`Udstedte faktura ${result.invoiceNumber ?? ""} for ${result.issueDate ?? asOfDate}.`,
				);
			} else {
				setNotice(
					`Eksisterende faktura ${result.invoiceNumber ?? ""} blev returneret — perioden var allerede genereret.`,
				);
			}
			setSavedAsOfDate(asOfDate);
			markDateSaved();
			onReload();
		} catch (err) {
			const e = err as { message?: string };
			setError(e?.message ?? "Genereringen kunne ikke gennemføres.");
		} finally {
			setBusy(false);
		}
	}

	/**
	 * Retire (deactivate) the template. Templates are append-only by schema:
	 * once retired they cannot be reactivated and identity/payload columns
	 * cannot be mutated. To change terms, the owner creates a new template
	 * — historical generations on the old template are preserved untouched.
	 */
	async function retire(reason: string) {
		if (outcome.isBlocked()) return;
		setRetireBusy(true);
		setError(null);
		setNotice(null);
		try {
			await outcome.run(() =>
				api.retireRecurringInvoiceTemplate(
					slug,
					template.id,
					reason && reason.trim().length > 0 ? reason.trim() : undefined,
				),
			);
			setNotice(`Skabelonen "${template.name}" er deaktiveret.`);
			setRetiring(false);
			onReload();
		} catch (err) {
			throw err;
		} finally {
			setRetireBusy(false);
		}
	}

	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
				viewStyles.site1,
			)}
		>
			{outcome.feedback}
			<h4
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h4,
					viewStyles.site2,
				)}
			>
				{template.name}{" "}
				{!template.active && (
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						(tilbagetrukken)
					</span>
				)}
			</h4>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{(template.intervalCount ?? 1) > 1
					? `hver ${template.intervalCount}. `
					: ""}
				{INTERVAL_LABELS[template.interval]} ·{" "}
				{CHANNEL_LABELS[template.deliveryChannel ?? "manual"]} · næste
				udstedelse {template.nextIssueDate} · betalingsfrist{" "}
				{template.paymentTermsDays} dage
				{template.notes ? ` · ${template.notes}` : ""}
			</p>

			{template.active && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
						viewStyles.site3,
					)}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Udsted som af
						<Input
							type="date"
							value={asOfDate}
							onChange={(e) => setAsOfDate(e.target.value)}
							disabled={outcome.blocked || busy || retireBusy}
							xstyle={[cockpitStyles.rowActionsInputComposition]}
						/>
					</label>
					<Button
						requiredPermission="company.draft.write"
						onClick={generate}
						disabled={
							outcome.blocked || busy || retireBusy || asOfDate.length !== 10
						}
						type="button"
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{busy ? "Genererer…" : "Generér"}
					</Button>
					<Button
						requiredPermission="company.draft.write"
						variant="secondary"
						onClick={() => setRetiring(true)}
						disabled={outcome.blocked || busy || retireBusy}
						type="button"
						aria-label={`Deaktivér skabelonen ${template.name}`}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{retireBusy ? "Deaktiverer…" : "Deaktivér"}
					</Button>
				</div>
			)}

			{!template.active && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
						viewStyles.site4,
					)}
				>
					Skabelonen er deaktiveret og kan ikke længere generere fakturaer.
					Tidligere genererede fakturaer (nedenfor) er bevaret uændret.
				</p>
			)}

			{retiring && (
				<ConfirmDialog
					title={`Deaktivér skabelonen ${template.name}?`}
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Skabelonen kan ikke genaktiveres og kan ikke generere flere
							fakturaer. Tidligere fakturaer og revisionshistorik bevares. Opret
							en ny skabelon, hvis beløb eller frekvens skal ændres.
						</p>
					}
					confirmLabel="Deaktivér skabelon"
					confirmKind="danger"
					noteLabel="Årsag (valgfri)"
					onConfirm={retire}
					onClose={() => setRetiring(false)}
					onRefresh={onReload}
				/>
			)}

			{error && <Banner kind="error">{error}</Banner>}
			{notice && <Banner kind="success">{notice}</Banner>}

			{template.generations.length > 0 && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.tableScroll,
						viewStyles.site5,
					)}
					data-ui="table-scroll"
				>
					<table
						aria-label="Udstedte fakturaer fra skabelonen"
						{...stylex.props(cockpitStyles.tableStatementTableComposition)}
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
										cockpitStyles.tableStatementTableThComposition,
									)}
								>
									Periode
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition,
									)}
								>
									Fakturanr.
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition,
									)}
								>
									Udstedt
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition,
									)}
								>
									Leveringsperiode
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
							{template.generations.map((g) => (
								<tr
									key={g.id}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}
								>
									<td
										{...stylex.props(
											cockpitStyles.tableStatementTableTdAccountNoComposition4,
										)}
									>
										#{g.periodIndex}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableStatementTableTdAccountNoComposition4,
										)}
									>
										{g.invoiceNumber}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableStatementTableTdComposition5,
										)}
									>
										{g.issueDate}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableStatementTableTdComposition6,
										)}
									>
										{g.deliveryPeriodStart && g.deliveryPeriodEnd
											? `${g.deliveryPeriodStart} → ${g.deliveryPeriodEnd}`
											: "—"}
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}
		</div>
	);
}

const viewStyles = stylex.create({
	site0: { gap: 8 },
	site1: { marginBottom: 16 },
	site2: { marginTop: 0 },
	site3: { alignItems: "center", gap: 12 },
	site4: { fontStyle: "italic" },
	site5: { marginTop: 12 },
});
