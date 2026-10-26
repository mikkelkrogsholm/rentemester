import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { useState } from "react";
import { RegisterAccrualModal, RecognizeAccrualModal } from "./AccrualModal";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatKroner } from "../lib/format";
import type { AccrualRegisterRow, CompanyAccrualsResponse } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const TYPE_LABEL: Record<AccrualRegisterRow["accrualType"], string> = {
	prepaid_expense: "Forudbetalt omkostning",
	accrued_expense: "Skyldig omkostning",
	deferred_revenue: "Udskudt omsætning",
};

export function AccrualsView() {
	const { slug = "" } = useParams();
	const [register, setRegister] = useState(false);
	const [recognize, setRecognize] = useState<AccrualRegisterRow | null>(null);
	const state = useAsync<CompanyAccrualsResponse["accruals"]>(
		(signal) => api.accruals(slug, { signal }),
		[slug],
	);

	if (state.loading) return <Loading />;
	if (state.error) return <ErrorState message={state.error} />;
	const data = state.data!;
	const r = data.report;
	const currency = data.company.currency || "DKK";

	return (
		<section {...stylex.props(...[cockpitStyles.statement])}>
			<PageHeader
				title="Periodisering"
				actions={
					<>
						<div {...stylex.props(cockpitStyles.rowActions)}>
							{data.nativeAccruals && (
								<Button onClick={() => setRegister(true)}>
									Ny periodisering
								</Button>
							)}
							<ButtonLink

								to={`/companies/${slug}/manage`}
							>
								Administrér
							</ButtonLink>
						</div>
					</>
				}
			>
				<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					<p {...stylex.props(cockpitStyles.muted)}>
						{data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
						{data.company.country} · {currency} · Periodisering
					</p>
				</div>
			</PageHeader>

			<p {...stylex.props(cockpitStyles.muted)}>
				Periodeafgrænsningsposter (PAP): forudbetalte omkostninger, skyldige
				omkostninger og udskudt omsætning. Beløb rulles ud lineært over de
				aftalte perioder. Registrering bogfører hele grundbeløbet; hver periode
				bogføres særskilt efter din bekræftelse.
			</p>

			<section {...stylex.props(cockpitStyles.card)}>
				<h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Portfolio</h3>
				<div {...stylex.props(cockpitStyles.filterBar)}>
					<span {...stylex.props(...[cockpitStyles.statusChip])}>
						I alt: {formatKroner(r.totals.totalAmount, currency)}
					</span>
					<span {...stylex.props(...[cockpitStyles.statusChip, cockpitStyles.statusChipSuccess])}>
						Realiseret: {formatKroner(r.totals.recognizedAmount, currency)}
					</span>
					<span {...stylex.props(...[cockpitStyles.statusChip, cockpitStyles.statusChipWarning])}>
						Tilbage: {formatKroner(r.totals.remainingAmount, currency)}
					</span>
				</div>
			</section>

			<section {...stylex.props(cockpitStyles.card)}>
				<h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Accruals ({r.accruals.length})</h3>
				{r.accruals.length === 0 ? (
					<p {...stylex.props(cockpitStyles.muted)}>Ingen accruals registreret.</p>
				) : (
					<div {...stylex.props(cockpitStyles.tableScroll)}>
						<table {...stylex.props(...[cockpitStyles.tableData])}>
							<thead {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableThead,
								)}>
								<tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Type</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Beskrivelse</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Total</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Realiseret</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Tilbage</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Perioder</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Første dato</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Balance / Result</th>
									<th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTh,
											cockpitStyles.responsiveTableThLastChild,
										)}>Status</th>
									<th>Handling</th>
								</tr>
							</thead>
							<tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTbody,
								)}>
								{r.accruals.map((a) => (
									<AccrualRow
										key={a.accrualId}
										row={a}
										currency={currency}
										onRecognize={
											data.nativeAccruals ? () => setRecognize(a) : undefined
										}
									/>
								))}
							</tbody>
						</table>
					</div>
				)}
			</section>
			{register && (
				<RegisterAccrualModal
					slug={slug}
					onClose={() => setRegister(false)}
					onSaved={() => {
						setRegister(false);
						state.reload();
					}}
				/>
			)}
			{recognize && (
				<RecognizeAccrualModal
					slug={slug}
					row={recognize}
					onClose={() => setRecognize(null)}
					onSaved={() => {
						setRecognize(null);
						state.reload();
					}}
				/>
			)}
		</section>
	);
}

function AccrualRow({
	row,
	currency,
	onRecognize,
}: {
	row: AccrualRegisterRow;
	currency: string;
	onRecognize?: () => void;
}) {
	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{TYPE_LABEL[row.accrualType]}</td>
			<td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.description}</td>
			<td {...stylex.props(cockpitStyles.num)}>{formatKroner(row.totalAmount, currency)}</td>
			<td {...stylex.props(cockpitStyles.num)}>{formatKroner(row.recognizedAmount, currency)}</td>
			<td {...stylex.props(cockpitStyles.num)}>{formatKroner(row.remainingAmount, currency)}</td>
			<td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
				{row.recognizedPeriods}/{row.recognitionPeriods} ×{" "}
				{row.periodStepMonths} mdr
			</td>
			<td {...stylex.props(cockpitStyles.entryDate)}>{row.firstRecognitionDate}</td>
			<td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
				<code {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}>{row.balanceAccountNo}</code> / <code {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}>{row.resultAccountNo}</code>
			</td>
			<td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
				{row.fullyRecognized ? (
					<span {...stylex.props(...[cockpitStyles.statusChip, cockpitStyles.statusChipSuccess])}>Fuldt realiseret</span>
				) : (
					<span {...stylex.props(...[cockpitStyles.statusChip])}>Aktiv</span>
				)}
			</td>
			<td>
				{!row.fullyRecognized && onRecognize && (
					<Button onClick={onRecognize}>
						Bogfør periode for {row.description}
					</Button>
				)}
			</td>
		</tr>
	);
}
