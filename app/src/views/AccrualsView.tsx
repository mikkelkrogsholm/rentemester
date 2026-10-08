import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Periodisering / accrual register (#337).
//
// Read-only view: per-virksomhed liste over registrerede accruals med
// recognized amount, remaining amount og portfolio-totals. Genbruger
// kernens `buildAccrualRegisterReport` direkte.
//
// Register-new-accrual + recognize-period write-flows er parkeret som
// follow-ups — kernens CLI (`accrual register` / `accrual recognize`)
// dækker dem indtil videre.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { PageState, StatusChip } from "../components/CockpitPrimitives";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { AccrualRegisterRow, CompanyAccrualsResponse } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const TYPE_LABEL: Record<AccrualRegisterRow["accrualType"], string> = {
	prepaid_expense: "Forudbetalt omkostning",
	accrued_expense: "Skyldig omkostning",
	deferred_revenue: "Udskudt omsætning",
};

export function AccrualsView() {
	const { slug = "" } = useParams();
	const state = useAsync<CompanyAccrualsResponse["accruals"]>(
		(signal) => api.accruals(slug, { signal }),
		[slug],
	);

	if (state.loading && !state.data)
		return <PageState kind="loading" title="Henter periodiseringer" />;
	if (state.error)
		return (
			<PageState
				kind="error"
				title="Periodiseringer kunne ikke hentes"
				onRetry={state.reload}
			>
				{state.error}
			</PageState>
		);
	const data = state.data!;
	const r = data.report;
	const currency = data.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="accruals"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Periodisering"
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
								xstyle={[cockpitStyles.accrualsViewBtnComposition]}
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
						{data.company.country} · {currency} · Periodisering
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
						Sikker næste handling
					</h3>
					<StatusChip tone="info">Kræver review</StatusChip>
				</div>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Rentemester bogfører ikke en periodisering fra denne side. Gennemgå
					først opgaven og brug derefter den eksisterende
					agent-/review-arbejdsgang.
				</p>
				<Link
					to={`/companies/${slug}/opmaerksomhed`}
					{...stylex.props(cockpitStyles.accrualsViewBtnComposition)}
				>
					Åbn opgaver der kræver opmærksomhed
				</Link>
				<CopySafeStep slug={slug} />
			</section>

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
					Portfolio
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
						I alt: {formatKroner(r.totals.totalAmount, currency)}
					</span>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Realiseret: {formatKroner(r.totals.recognizedAmount, currency)}
					</span>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Tilbage: {formatKroner(r.totals.remainingAmount, currency)}
					</span>
				</div>
			</section>

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
					Accruals ({r.accruals.length})
				</h3>
				{r.accruals.length === 0 ? (
					<PageState kind="empty" title="Ingen periodiseringer registreret">
						Ingen accruals registreret. Åbn opgaver der kræver opmærksomhed og
						kopiér den sikre næste handling. Den opretter eller bogfører intet.
					</PageState>
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
							aria-label="Periodiseringer"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTable,
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
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Type
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Beskrivelse
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Total
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Realiseret
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Tilbage
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Perioder
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Første dato
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Balance / Result
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}
									>
										Status
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
								{r.accruals.map((a) => (
									<AccrualRow key={a.accrualId} row={a} currency={currency} />
								))}
							</tbody>
						</table>
					</div>
				)}
			</section>
		</section>
	);
}

function CopySafeStep({ slug }: { slug: string }) {
	const [copied, setCopied] = useState(false);
	const text = `Åbn Cockpit → ${slug} → Opgaver der kræver opmærksomhed. Gennemgå bilag og forslag, lav dry run og bekræft først derefter en periodisering.`;
	return (
		<Button
			type="button"
			onClick={async () => {
				try {
					await navigator.clipboard.writeText(text);
					setCopied(true);
				} catch {
					/* visible text remains safe */
				}
			}}
			variant={"secondary"}
			xstyle={[cockpitStyles.buttonComposition]}
		>
			{copied ? "Kopieret" : "Kopiér sikker næste handling"}
		</Button>
	);
}

function AccrualRow({
	row,
	currency,
}: {
	row: AccrualRegisterRow;
	currency: string;
}) {
	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{TYPE_LABEL[row.accrualType]}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.description}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.num,
					cockpitStyles.tdNum,
				)}
			>
				{formatKroner(row.totalAmount, currency)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.num,
					cockpitStyles.tdNum,
				)}
			>
				{formatKroner(row.recognizedAmount, currency)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.num,
					cockpitStyles.tdNum,
				)}
			>
				{formatKroner(row.remainingAmount, currency)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.recognizedPeriods}/{row.recognitionPeriods} ×{" "}
				{row.periodStepMonths} mdr
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.entryDate,
				)}
			>
				{row.firstRecognitionDate}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<code
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}
				>
					{row.balanceAccountNo}
				</code>{" "}
				/{" "}
				<code
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}
				>
					{row.resultAccountNo}
				</code>
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.fullyRecognized ? (
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Fuldt realiseret
					</span>
				) : (
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Aktiv
					</span>
				)}
			</td>
		</tr>
	);
}
