import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader, Select } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Årsrapport-builder UI (#338).
//
// Per-virksomhed view der bygger en regnskabsklasse-B-årsrapport for et
// regnskabsår. Brugeren angiver start + slut; resten kommer fra kernens
// `buildAnnualReport` (samme funktion som CLI'ens `report annual`).
// Forudsætnings-fejl (CVR mangler, periode er ikke låst, bøgerne
// balancerer ikke) vises tydeligt så ejeren kan rette dem og prøve igen.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { StatusChip } from "../components/CockpitPrimitives";
import { ApiError, api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { AnnualReport, CompanyAnnualReportResponse } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function AnnualReportView() {
	const { slug = "" } = useParams();
	const today = new Date();
	const defaultYear = today.getUTCFullYear() - 1;
	const [selectedYear, setSelectedYear] = useState(String(defaultYear));
	const readiness = useAsync(
		(signal) => api.annualReport(slug, "", "", selectedYear, { signal }),
		[slug, selectedYear],
	);
	const [data, setData] = useState<
		CompanyAnnualReportResponse["annualReport"] | null
	>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const build = async (e: React.FormEvent) => {
		e.preventDefault();
		setError(null);
		setLoading(true);
		try {
			const result = await api.annualReport(slug, "", "", selectedYear);
			setData(result);
		} catch (err) {
			setError(
				err instanceof ApiError ? err.message : "Kunne ikke bygge årsrapport.",
			);
			setData(null);
		} finally {
			setLoading(false);
		}
	};

	return (
		<section
			data-cockpit-page="annual-report"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Årsrapport"
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
								xstyle={[cockpitStyles.annualReportViewBtnComposition]}
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
						Forbereder en årsrapport for et lukket regnskabsår. Rentemester
						samler resultatopgørelse, balance og noter — den endelige aflæsning
						sker hos revisor.
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
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Vælg regnskabsår
				</h3>
				<form
					onSubmit={build}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.filterBar,
					)}
				>
					<label
						htmlFor="annual-year"
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}
					>
						Regnskabsår
						<Select
							id="annual-year"
							value={selectedYear}
							onChange={(e) => setSelectedYear(e.target.value)}
							xstyle={[cockpitStyles.filterBarSelectComposition2]}
						>
							{[0, 1, 2, 3, 4].map((offset) => (
								<option
									key={defaultYear - offset}
									value={defaultYear - offset}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{defaultYear - offset}
								</option>
							))}
						</Select>
					</label>
					<Button
						type="submit"
						disabled={
							loading ||
							readiness.loading ||
							readiness.data?.readiness?.status === "Ikke klar"
						}
						title={
							readiness.data?.readiness?.status === "Ikke klar"
								? "Ret readiness-kontrollerne før build"
								: undefined
						}
						xstyle={[cockpitStyles.annualReportViewBtnComposition2]}
					>
						{loading ? "Bygger …" : "Byg årsrapport"}
					</Button>
				</form>
			</section>

			{error && (
				<div
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</div>
			)}

			{readiness.data && (
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
							Readiness
						</h3>
						<StatusChip
							tone={
								readiness.data.readiness?.status === "Klar"
									? "success"
									: "danger"
							}
						>
							{readiness.data.readiness?.status ?? "Ikke klar"}
						</StatusChip>
					</div>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Canoniske datoer: {readiness.data.fiscalYearStart} –{" "}
						{readiness.data.fiscalYearEnd}
					</p>
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{(readiness.data.readiness?.items ?? []).map((item) => (
							<li
								key={item.label}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{item.ok ? "✓" : "•"} {item.label}
								{!item.ok && (
									<>
										{" "}
										—{" "}
										<Link
											to={`/companies/${slug}/${item.destination === "periods" ? "perioder" : "manage"}`}
											{...stylex.props(cockpitStyles.aComposition)}
										>
											Åbn løsning
										</Link>
									</>
								)}
							</li>
						))}
					</ul>
				</section>
			)}

			{data && <ReportPanel report={data.report} />}
		</section>
	);
}

function ReportPanel({ report }: { report: AnnualReport }) {
	if (!report.ok) {
		return (
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
					Forudsætninger mangler
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					For at en årsrapport kan dannes skal: virksomhedens CVR være
					registreret, regnskabsåret være lukket under{" "}
					<Link to="../periods" {...stylex.props(cockpitStyles.aComposition)}>
						Periodelås
					</Link>
					, og bøgerne skal balancere.
				</p>
				<ul
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{report.errors.map((err, i) => (
						<li
							key={i}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{err}
						</li>
					))}
				</ul>
			</section>
		);
	}
	const currency = report.company.currency || "DKK";
	return (
		<>
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
					Stamdata pr. {report.fiscalYearStart} — {report.fiscalYearEnd}
				</h3>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.tableScroll,
					)}
					data-ui="table-scroll"
				>
					<table
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
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
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Virksomhed
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{report.company.name}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									CVR
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{report.company.cvr ?? "—"}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Land
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{report.company.country}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Valuta
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{report.company.currency}
								</td>
							</tr>
						</tbody>
					</table>
				</div>
			</section>

			{report.profitAndLoss && (
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
						Resultatopgørelse
					</h3>
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
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Indtægter i alt
									</th>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.num,
											cockpitStyles.tdNum,
										)}
									>
										{formatKroner(report.profitAndLoss.income.total, currency)}
									</td>
								</tr>
								<tr
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Omkostninger i alt
									</th>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.num,
											cockpitStyles.tdNum,
										)}
									>
										{formatKroner(report.profitAndLoss.expense.total, currency)}
									</td>
								</tr>
								<tr
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Årets resultat
									</th>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.num,
											cockpitStyles.tdNum,
											cockpitStyles.statementResultTd,
										)}
									>
										{formatKroner(report.profitAndLoss.result, currency)}
									</td>
								</tr>
							</tbody>
						</table>
					</div>
				</section>
			)}

			{report.balanceSheet && (
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
						Balance ultimo {report.fiscalYearEnd}
					</h3>
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
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Aktiver i alt
									</th>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.num,
											cockpitStyles.tdNum,
										)}
									>
										{formatKroner(report.balanceSheet.assets.total, currency)}
									</td>
								</tr>
								<tr
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Gæld i alt
									</th>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.num,
											cockpitStyles.tdNum,
										)}
									>
										{formatKroner(
											report.balanceSheet.liabilities.total,
											currency,
										)}
									</td>
								</tr>
								<tr
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Egenkapital i alt
									</th>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.num,
											cockpitStyles.tdNum,
										)}
									>
										{formatKroner(report.balanceSheet.equity.total, currency)}
									</td>
								</tr>
							</tbody>
						</table>
					</div>
				</section>
			)}

			{report.notes && report.notes.length > 0 && (
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
						Noter
					</h3>
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{report.notes.map((n) => (
							<li
								key={n.id}
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
									{n.title}
								</strong>
								<p
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{n.body}
								</p>
							</li>
						))}
					</ul>
				</section>
			)}

			{report.ledelsespategning && (
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
						Ledelsespåtegning
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Dato: {report.ledelsespategning.date}
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{report.ledelsespategning.body}
					</p>
				</section>
			)}
		</>
	);
}
