import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// GDPR export + forget UI (#334).
//
// Per-virksomhed view der hjælper ejeren med at besvare en indsigtsanmodning
// (Persondataforordningens art. 15) og udføre en sletning (art. 17). Begge
// flows er tynde skaller over kernens buildGdprSubjectExport og
// eraseGdprSubject — kernen håndterer 5-års retention og audit-log'ing.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ApiError, api } from "../lib/api";
import type {
	CompanyGdpr,
	GdprErasureResult,
	GdprExportRecord,
} from "../lib/types";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationOutcome } from "../lib/useMutationOutcome";

const SOURCE_LABEL: Record<string, string> = {
	customers: "Kunde",
	vendors: "Leverandør",
	documents: "Bilag",
	bank_transactions: "Banktransaktion",
	journal_entries: "Postering",
	journal_lines: "Posteringslinje",
	audit_log: "Audit-log",
};

export function GdprView() {
	const { slug = "" } = useParams();
	const { can } = useCapabilities(slug);
	const [cvr, setCvr] = useState("");
	const [name, setName] = useState("");
	const [exportData, setExportData] = useState<CompanyGdpr | null>(null);
	const [erasure, setErasure] = useState<GdprErasureResult | null>(null);
	const [loading, setLoading] = useState(false);
	const [erasing, setErasing] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [pendingErase, setPendingErase] = useState(false);
	const [reviewedSubject, setReviewedSubject] = useState<{
		cvr?: string;
		name?: string;
	} | null>(null);
	const refreshExport = async () => {
		if (!reviewedSubject) return;
		const data = await api.gdprExport(slug, reviewedSubject);
		setExportData(data);
	};
	// Even a GDPR lookup appends an attributed audit event. It is never used as
	// a read-only retry/status button after an interrupted write.
	const outcome = useMutationOutcome();

	const runExport = async (e: React.FormEvent) => {
		e.preventDefault();
		if (loading || outcome.isBlocked() || !can("company.export")) return;
		const subject = {
			cvr: cvr.trim() || undefined,
			name: name.trim() || undefined,
		};
		setError(null);
		setErasure(null);
		setExportData(null);
		setReviewedSubject(null);
		setLoading(true);
		try {
			const data = await outcome.run(() => api.gdprExport(slug, subject));
			setReviewedSubject(subject);
			setExportData(data);
		} catch (err) {
			setError(
				err instanceof ApiError ? err.message : "Indsigtsopslag fejlede.",
			);
			setExportData(null);
		} finally {
			setLoading(false);
		}
	};

	const runErase = async () => {
		if (
			!exportData ||
			!reviewedSubject ||
			outcome.isBlocked() ||
			!can("company.admin")
		)
			return;
		setError(null);
		setErasing(true);
		try {
			const result = await outcome.run(() =>
				api.gdprErase(slug, reviewedSubject),
			);
			setErasure(result);
			// Re-run export så ejeren ser den opdaterede status.
			try {
				await refreshExport();
			} catch {
				setError(
					"Anonymiseringen er gennemført, men den opdaterede indsigt kunne ikke hentes.",
				);
			}
		} catch (err) {
			setError(
				err instanceof ApiError ? err.message : "Anonymisering fejlede.",
			);
			throw err;
		} finally {
			setErasing(false);
		}
	};

	return (
		<section
			data-cockpit-page="gdpr"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			{outcome.feedback}
			<PageHeader
				title="GDPR-indsigt"
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
								xstyle={[cockpitStyles.gdprViewBtnComposition]}
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
						Find personoplysninger om en person eller virksomhed (kunde eller
						leverandør) og anonymisér dem hvor bogføringspligten ikke længere
						kræver dem.
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
					Find oplysninger
				</h3>
				<form
					onSubmit={runExport}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.filterBar,
					)}
				>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}
					>
						CVR
						<Input
							disabled={loading || erasing || outcome.blocked}
							type="text"
							value={cvr}
							onChange={(e) => {
								setCvr(e.target.value);
								setExportData(null);
								setReviewedSubject(null);
								setErasure(null);
							}}
							placeholder="DK…"
							xstyle={[cockpitStyles.filterBarInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}
					>
						Navn
						<Input
							disabled={loading || erasing || outcome.blocked}
							type="text"
							value={name}
							onChange={(e) => {
								setName(e.target.value);
								setExportData(null);
								setReviewedSubject(null);
								setErasure(null);
							}}
							placeholder="fx 'Acme ApS'"
							xstyle={[cockpitStyles.filterBarInputComposition]}
						/>
					</label>
					<Button
						requiredPermission="company.export"
						type="submit"
						disabled={
							outcome.blocked || loading || (!cvr.trim() && !name.trim())
						}
						xstyle={[cockpitStyles.gdprViewBtnComposition2]}
					>
						{loading ? "Søger …" : "Find oplysninger"}
					</Button>
				</form>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Mindst ét felt er påkrævet. Navnesøgning skelner mellem store og små
					bogstaver.
				</p>
			</section>

			{error && (
				<div
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</div>
			)}

			{exportData && (
				<ExportPanel
					data={exportData}
					erasing={erasing || outcome.blocked}
					onErase={() => setPendingErase(true)}
				/>
			)}

			{erasure && <ErasureSummary result={erasure} />}

			{pendingErase && (
				<ConfirmDialog
					title="Bekræft anonymisering"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Anonymisér det gennemgåede subjekt{" "}
							{reviewedSubject?.cvr || reviewedSubject?.name}. Konsekvens:
							Anonymisering erstatter de viste, tilladte personoplysninger med
							en spærret markering. Rækker, der stadig er bogføringspligtige,
							springes over. Handlingen kan ikke fortrydes.
						</p>
					}
					confirmLabel="Anonymisér nu"
					confirmKind="danger"
					onConfirm={async () => {
						await runErase();
					}}
					onClose={() => setPendingErase(false)}
				/>
			)}
		</section>
	);
}

function ExportPanel({
	data,
	erasing,
	onErase,
}: {
	data: CompanyGdpr;
	erasing: boolean;
	onErase: () => void;
}) {
	const { records } = data.export;
	const underRetention = records.filter((r) => r.underRetention).length;
	const erasable = records.filter((r) => r.erasable).length;
	const alreadyErased = records.filter((r) => r.erased).length;

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
				Indsigtsrapport ({records.length} række
				{records.length === 1 ? "" : "r"})
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Hentet pr. {data.export.asOf}. Under bogføringspligt: {underRetention}.
				Allerede anonymiseret: {alreadyErased}. Kan anonymiseres nu: {erasable}.
			</p>
			{records.length === 0 ? (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Ingen personoplysninger fundet for det angivne navn/CVR.
				</p>
			) : (
				<>
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
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Kilde
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Navn
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										CVR
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Email
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Adresse
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Opbevares til
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
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
								)}
							>
								{records.map((r, i) => (
									<RecordRow
										key={`${r.source}-${r.sourceRowId}-${i}`}
										row={r}
									/>
								))}
							</tbody>
						</table>
					</div>
					<section
						aria-labelledby="anonymisering-heading"
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.card,
						)}
					>
						<h4
							id="anonymisering-heading"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h4,
							)}
						>
							Anonymisering
						</h4>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Resultat: {erasable} række{erasable === 1 ? " kan" : "r kan"}{" "}
							anonymiseres nu, mens {underRetention} fortsat skal opbevares.
						</p>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Konsekvens: Tilladte personoplysninger erstattes permanent med en
							spærret markering. Gennemgå resultatet ovenfor før du fortsætter.
						</p>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							<Button
								type="button"
								variant="danger"
								requiredPermission="company.admin"
								onClick={onErase}
								disabled={erasing || erasable === 0}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								{erasing
									? "Anonymiserer …"
									: `Anonymisér de ${erasable} mulige rækker`}
							</Button>
						</div>
					</section>
				</>
			)}
		</section>
	);
}

function RecordRow({ row }: { row: GdprExportRecord }) {
	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{SOURCE_LABEL[row.source] ?? row.source}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.personalData.name ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.personalData.vatOrCvr ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.personalData.email ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{row.personalData.address ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{row.retainUntil ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.erased ? (
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Anonymiseret
					</span>
				) : row.underRetention ? (
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Under bogføringspligt
					</span>
				) : !row.erasable ? (
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kan ikke anonymiseres
					</span>
				) : (
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kan anonymiseres
					</span>
				)}
			</td>
		</tr>
	);
}

function ErasureSummary({ result }: { result: GdprErasureResult }) {
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
				Anonymisering — resultat
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Pr. {result.asOf}. Anonymiseret: {result.erasedCount}. Allerede
				anonymiseret: {result.alreadyErasedCount}. Afvist (under
				bogføringspligt): {result.refusedCount}.
			</p>
			{result.refused.length > 0 && (
				<>
					<h4
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h4,
						)}
					>
						Afviste rækker
					</h4>
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
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Kilde
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Reference
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Opbevares til
									</th>
									<th
										{...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Grund
									</th>
								</tr>
							</thead>
							<tbody
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{result.refused.map((r, i) => (
									<tr
										key={i}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{SOURCE_LABEL[r.source] ?? r.source}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{r.label ?? `#${r.sourceRowId}`}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											{r.retainUntil ?? "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{r.reason}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
				</>
			)}
		</section>
	);
}
