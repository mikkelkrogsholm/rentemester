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
		<section className="accruals-view">
			<PageHeader
				title="Periodisering"
				actions={
					<>
						<div className="row-actions">
							{data.nativeAccruals && (
								<Button onClick={() => setRegister(true)}>
									Ny periodisering
								</Button>
							)}
							<ButtonLink
								className="btn secondary"
								to={`/companies/${slug}/manage`}
							>
								Administrér
							</ButtonLink>
						</div>
					</>
				}
			>
				<div>
					<p className="muted">
						{data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
						{data.company.country} · {currency} · Periodisering
					</p>
				</div>
			</PageHeader>

			<p className="muted">
				Periodeafgrænsningsposter (PAP): forudbetalte omkostninger, skyldige
				omkostninger og udskudt omsætning. Beløb rulles ud lineært over de
				aftalte perioder. Registrering bogfører hele grundbeløbet; hver periode
				bogføres særskilt efter din bekræftelse.
			</p>

			<section className="card">
				<h3>Portfolio</h3>
				<div className="filter-bar">
					<span className="pill">
						I alt: {formatKroner(r.totals.totalAmount, currency)}
					</span>
					<span className="pill ok">
						Realiseret: {formatKroner(r.totals.recognizedAmount, currency)}
					</span>
					<span className="pill warn">
						Tilbage: {formatKroner(r.totals.remainingAmount, currency)}
					</span>
				</div>
			</section>

			<section className="card">
				<h3>Accruals ({r.accruals.length})</h3>
				{r.accruals.length === 0 ? (
					<p className="muted">Ingen accruals registreret.</p>
				) : (
					<div className="table-scroll">
						<table className="table">
							<thead>
								<tr>
									<th>Type</th>
									<th>Beskrivelse</th>
									<th>Total</th>
									<th>Realiseret</th>
									<th>Tilbage</th>
									<th>Perioder</th>
									<th>Første dato</th>
									<th>Balance / Result</th>
									<th>Status</th>
									<th>Handling</th>
								</tr>
							</thead>
							<tbody>
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
		<tr>
			<td>{TYPE_LABEL[row.accrualType]}</td>
			<td>{row.description}</td>
			<td className="num">{formatKroner(row.totalAmount, currency)}</td>
			<td className="num">{formatKroner(row.recognizedAmount, currency)}</td>
			<td className="num">{formatKroner(row.remainingAmount, currency)}</td>
			<td>
				{row.recognizedPeriods}/{row.recognitionPeriods} ×{" "}
				{row.periodStepMonths} mdr
			</td>
			<td className="entry-date">{row.firstRecognitionDate}</td>
			<td>
				<code>{row.balanceAccountNo}</code> / <code>{row.resultAccountNo}</code>
			</td>
			<td>
				{row.fullyRecognized ? (
					<span className="pill ok">Fuldt realiseret</span>
				) : (
					<span className="pill">Aktiv</span>
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
