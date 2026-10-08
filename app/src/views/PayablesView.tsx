import * as stylex from "@stylexjs/stylex";
import { ErrorState } from "../components/Feedback";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Leverandørfaktura-arbejdsbordet (#340) — the cockpit's payable workbench.
//
// Renders `/api/companies/:slug/payables`: the kreditorliste from
// `core/payables.ts#buildPayablesList` plus the picker rows the
// "Registrér leverandørfaktura"-modal needs. Summary cards above the table
// give the total open balance, the overdue portion and the not-yet-due
// portion; status filter pills switch between Åbne / Forfaldne / Betalte /
// Alle.
//
// Two write actions live on this view:
//   - "Registrér leverandørfaktura" (page action) opens the
//     `PayableRegisterModal`, turning an ingested bilag into a kreditorpost.
//   - per row, "Markér betalt" opens a `ConfirmDialog` that takes a bank
//     transaction id and runs `api.payPayable` — the same `payPayableFromBank`
//     core function the CLI's `payable pay` command uses.
//
// All write actions are write-irreversible and go through the same
// `withCompanyMutation` pipeline as every other cockpit write — confirm gate,
// backup lock and actor attribution included.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { FilterBar, PageState } from "../components/CockpitPrimitives";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { DirectBankPayableCorrectionModal } from "../components/DirectBankPayableCorrectionModal";
import { LegacyPayableBackfillModal } from "../components/LegacyPayableBackfillModal";
import { PayableRegisterModal } from "../components/PayableRegisterModal";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type {
	CompanyPayableRow,
	CompanyPayables,
	PayableListStatusFilter,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

const FILTERS: { value: PayableListStatusFilter; label: string }[] = [
	{ value: "open", label: "Åbne" },
	{ value: "overdue", label: "Forfaldne" },
	{ value: "paid", label: "Betalte" },
	{ value: "all", label: "Alle" },
];

const AGING_LABELS: Record<CompanyPayableRow["agingBucket"], string> = {
	"not-due": "Ikke forfalden",
	"0-30": "Forfalden 1–30 dage",
	"31-60": "Forfalden 31–60 dage",
	"61-90": "Forfalden 61–90 dage",
	"90+": "Forfalden > 90 dage",
};

export function PayablesView() {
	const { slug = "" } = useParams();
	const { setYear } = useCompanyYear();
	const [filter, setFilter] = useState<PayableListStatusFilter>("open");
	const state = useAsync<CompanyPayables>(
		(signal) => api.payables(slug, filter, undefined, { signal }),
		[slug, filter],
	);
	const [registering, setRegistering] = useState(false);
	const [paying, setPaying] = useState<CompanyPayableRow | null>(null);
	const [correcting, setCorrecting] = useState(false);
	const [legacyBackfill, setLegacyBackfill] = useState(false);

	if (state.loading && !state.data) {
		return <PageState kind="loading" title="Henter leverandørfakturaer" />;
	}
	if (state.error && !state.data) {
		return <ErrorState message={state.error} onRetry={state.reload} />;
	}

	const view = state.data!;
	const currency = view.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="payables"
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
				title="Leverandørfakturaer"
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
								requiredPermission="company.ledger.post"
								type="button"
								onClick={() => setRegistering(true)}
								xstyle={[cockpitStyles.statementBtnComposition]}
							>
								Registrér leverandørfaktura
							</Button>
							<Button
								requiredPermission="company.ledger.post"
								variant="secondary"
								type="button"
								disabled={view.unregisteredDocuments.length === 0}
								onClick={() => setCorrecting(true)}
								xstyle={[cockpitStyles.statementBtnComposition]}
							>
								Ret direkte bankkøb
							</Button>
							<Button
								requiredPermission="company.admin"
								variant="secondary"
								type="button"
								onClick={() => setLegacyBackfill(true)}
								xstyle={[cockpitStyles.statementBtnComposition]}
							>
								Legacy kreditor-backfill
							</Button>
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
						{view.company.cvr ? `CVR ${view.company.cvr} · ` : ""}
						{view.company.country} · {currency} · Leverandørfakturaer
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={view.fiscalYears}
				// Payables are not bound to a fiscal year — show the first available
				// label for context; switching just rewrites the URL `?year=` param.
				selectedYear={view.fiscalYears[0]?.label ?? ""}
				onYearChange={setYear}
			/>

			{registering && (
				<PayableRegisterModal
					slug={slug}
					payables={view}
					onRegistered={state.reload}
					onClose={() => setRegistering(false)}
				/>
			)}

			{correcting && (
				<DirectBankPayableCorrectionModal
					slug={slug}
					payables={view}
					onApplied={state.reload}
					onClose={() => setCorrecting(false)}
				/>
			)}
			{legacyBackfill && (
				<LegacyPayableBackfillModal
					slug={slug}
					onApplied={state.reload}
					onClose={() => setLegacyBackfill(false)}
				/>
			)}

			{paying && (
				<ConfirmDialog
					title="Markér leverandørfaktura betalt"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Markér{" "}
							<strong
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								#{paying.payableId}
							</strong>{" "}
							{paying.supplierName ? `til ${paying.supplierName}` : ""} betalt
							ved at angive id'et på den udgående banktransaktion. Posten lægger
							en afregningspostering (debet Leverandørgæld, kredit bank) og kan
							ikke fortrydes.
						</p>
					}
					confirmLabel="Markér betalt"
					confirmKind="danger"
					noteLabel="Banktransaktions-id"
					notePlaceholder="Det numeriske id på banklinjen"
					onConfirm={async (raw) => {
						const bankTransactionId = Number(raw.trim());
						if (
							!Number.isInteger(bankTransactionId) ||
							bankTransactionId <= 0
						) {
							throw {
								code: "bad_request",
								message:
									"Angiv det numeriske id på den udgående banktransaktion.",
							};
						}
						await api.payPayable(slug, {
							payableId: paying.payableId,
							bankTransactionId,
						});
						state.reload();
					}}
					onClose={() => setPaying(null)}
					onRefresh={state.reload}
				/>
			)}

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
						Skyldig i alt
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
							view.totalOpenBalance > 0 &&
								cockpitStyles.statusFigureStatusAlert,
						)}
					>
						{formatKroner(view.totalOpenBalance, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						Pr. {view.asOfDate} · {view.count}{" "}
						{view.count === 1 ? "post" : "poster"}
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
						Forfaldne
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
							view.overdueOpenBalance > 0 &&
								cockpitStyles.statusFigureStatusAlert,
						)}
					>
						{formatKroner(view.overdueOpenBalance, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						{view.overdueOpenBalance > 0
							? "Betal eller indgå aftale snart"
							: "Ingen forfaldne kreditorposter"}
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
						Ikke forfaldne
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(view.notYetDueOpenBalance, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						Skyldig, men ikke endnu forfalden
					</p>
				</div>
			</div>

			<FilterBar
				activeFilters={
					filter !== "open"
						? [
								`Status: ${FILTERS.find((item) => item.value === filter)?.label}`,
							]
						: []
				}
				onReset={() => setFilter("open")}
			>
				<nav
					aria-label="Filtrér leverandørfakturaer på status"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{FILTERS.map((f) => (
						<Button
							key={f.value}
							type="button"
							onClick={() => setFilter(f.value)}
							aria-pressed={filter === f.value}
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							{f.label}
						</Button>
					))}
				</nav>
			</FilterBar>

			{view.rows.length === 0 ? (
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
						Ingen leverandørfakturaer i visningen
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{filter === "open"
							? 'Der er ingen åbne kreditorposter at handle på. Brug "Registrér leverandørfaktura" når en ny købsfaktura er læst ind på Bilag-siden.'
							: filter === "overdue"
								? "Ingen forfaldne kreditorposter — godt arbejde."
								: filter === "paid"
									? "Ingen betalte kreditorposter endnu."
									: "Ingen kreditorposter er registreret endnu."}
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
						aria-label="Leverandørfakturaer"
						{...stylex.props(
							cockpitStyles.statementTableScrollTableComposition2,
						)}
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
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Leverandør
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Bilag
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Bilagsdato
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Forfald
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Brutto
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Betalt
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Åben
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Status
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
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
							{view.rows.map((row) => {
								const tone =
									row.status === "paid"
										? "ok"
										: row.isOverdue
											? "critical"
											: "neutral";
								const label =
									row.status === "paid"
										? "Betalt"
										: row.isOverdue
											? `Forfalden · ${row.overdueDays} dage`
											: "Bogført";
								const canPay = row.status === "open" && row.openBalance > 0;
								return (
									<tr
										key={row.payableId}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTr,
										)}
									>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition7,
											)}
										>
											{row.supplierName ?? "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableStatementTableTdAccountNoComposition3,
											)}
										>
											{row.billNo ?? `#${row.documentId}`}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition6,
											)}
										>
											{row.billDate}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition6,
											)}
										>
											{row.dueDate}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition6,
											)}
										>
											{formatKroner(row.grossAmount, currency)}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition6,
											)}
										>
											{row.paidAmount > 0
												? formatKroner(row.paidAmount, currency)
												: "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition6,
											)}
										>
											{row.openBalance > 0
												? formatKroner(row.openBalance, currency)
												: "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition7,
											)}
										>
											<span
												title={AGING_LABELS[row.agingBucket]}
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.flag,
													tone === "critical" && cockpitStyles.flagCritical,
													tone === "ok" && cockpitStyles.flagOk,
													tone === "neutral" && cockpitStyles.flagNeutral,
												)}
											>
												{label}
											</span>
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition7,
											)}
										>
											<div
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.rowActions,
												)}
											>
												{canPay && (
													<Button
														requiredPermission="company.ledger.post"
														variant="secondary"
														type="button"
														onClick={() => setPaying(row)}
														xstyle={[cockpitStyles.statementBtnComposition]}
													>
														Markér betalt
													</Button>
												)}
											</div>
										</td>
									</tr>
								);
							})}
						</tbody>
					</table>
				</div>
			)}
		</section>
	);
}
