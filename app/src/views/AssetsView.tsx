import * as stylex from "@stylexjs/stylex";
import {
	Button,
	ButtonLink,
	Dialog,
	Input,
	MoneyInput,
	PageHeader,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { parseDanishAmount } from "../lib/format";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
// Anlægskartotek — the per-company fixed-asset view (#336).
//
// Renders `/api/companies/:slug/assets`: every capitalised asset with its
// posted/remaining periods, akkumuleret afskrivning, restværdi and status,
// plus the straksafskrivning history. Three actions live here:
//
//   * "Registrér anlæg" — POST /api/companies/:slug/assets. The owner picks
//     an existing bilag (purchase document), enters acquisition date, cost,
//     levetid and category; the server computes the deterministic linear
//     depreciation plan via the SAME `registerAsset` core the CLI uses.
//
//   * "Beregn afskrivning" pr. række — POST .../assets/:id/depreciate.
//     A confirm-gated one-click that posts the NEXT unposted period of the
//     asset's schedule through `postDepreciationPeriod`. The cockpit shows
//     the period number + amount before the owner confirms.
//
//   * "Straksafskriv" — POST .../assets/write-off. Books a small purchase as
//     a straksafskrivning via `postImmediateWriteOff`; the threshold-rule
//     reference is captured verbatim on the audit record.
//
// All depreciation arithmetic is computed server-side — this view never
// re-implements the schedule.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { ApiError, api } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import type {
	AssetRow,
	AssetWriteOffRow,
	CompanyAssets,
	CompanyDocuments,
	DocumentRow,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

const DEFAULT_EXPENSE_ACCOUNT = "3000";
const DEFAULT_THRESHOLD_RULE =
	"AL §6 stk. 1 nr. 2 — småanskaffelser (straksafskrivning)";

export function AssetsView() {
	const { slug = "" } = useParams();
	const state = useAsync<CompanyAssets>(
		(signal) => api.assets(slug, { signal }),
		[slug],
	);
	const [registerOpen, setRegisterOpen] = useState(false);
	const [writeOffOpen, setWriteOffOpen] = useState(false);
	const [actionError, setActionError] = useState<string | null>(null);

	if (state.loading && !state.data)
		return <Loading label="Henter anlægskartotek…" />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const data = state.data!;
	const currency = data.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="assets"
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
				title="Anlæg"
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
						{data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
						{data.company.country} · {currency} · Anlæg
					</p>
				</div>
			</PageHeader>

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}
			>
				Anlægskartoteket — kapitaliserede aktiver, deres afskrivninger og
				straksafskrivninger. Alle beløb i {currency}.
			</p>

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
						Bogført kostpris
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(data.totals.cost, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						{data.totals.activeCount} aktive ·{" "}
						{data.totals.fullyDepreciatedCount} fuldt afskrevne
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
						Restværdi (netto)
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(data.totals.netBookValue, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						Akkumuleret afskrevet{" "}
						{formatKroner(data.totals.accumulatedDepreciation, currency)}
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
						Straksafskrivninger
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(data.totals.writeOffTotal, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						{data.totals.writeOffCount}{" "}
						{data.totals.writeOffCount === 1 ? "post" : "poster"}
					</p>
				</div>
			</div>

			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.rowActions,
					viewStyles.site0,
				)}
			>
				<Button
					requiredPermission="company.ledger.post"
					type="button"
					onClick={() => {
						setActionError(null);
						setRegisterOpen(true);
					}}
					xstyle={[cockpitStyles.statementBtnComposition]}
				>
					Registrér anlæg
				</Button>
				<Button
					requiredPermission="company.ledger.post"
					variant="secondary"
					type="button"
					onClick={() => {
						setActionError(null);
						setWriteOffOpen(true);
					}}
					xstyle={[cockpitStyles.statementBtnComposition]}
				>
					Straksafskriv
				</Button>
			</div>

			{actionError ? (
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
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{actionError}
					</p>
				</div>
			) : null}

			<h3
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					viewStyles.site1,
				)}
			>
				Kapitaliserede anlæg
			</h3>
			{data.assets.length === 0 ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Der er ingen kapitaliserede anlæg endnu. Brug "Registrér anlæg" til
						at oprette et nyt aktiv ud fra et eksisterende bilag.
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
						aria-label="Kapitaliserede anlæg"
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
									Navn
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Kategori
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Anskaffet
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Kostpris
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Akkumuleret afskrivning
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Restværdi
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
									Handling
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
							{data.assets.map((row) => (
								<AssetRowView
									key={row.assetId}
									row={row}
									slug={slug}
									currency={currency}
									onPosted={() => state.reload()}
									onError={setActionError}
								/>
							))}
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<td
									colSpan={3}
									{...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition8,
									)}
								>
									I alt
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdNumComposition7)}>
									{formatKroner(data.totals.cost, currency)}
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdNumComposition7)}>
									{formatKroner(data.totals.accumulatedDepreciation, currency)}
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdNumComposition7)}>
									{formatKroner(data.totals.netBookValue, currency)}
								</td>
								<td
									colSpan={2}
									{...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition8,
									)}
								/>
							</tr>
						</tbody>
					</table>
				</div>
			)}

			<h3
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					viewStyles.site2,
				)}
			>
				Straksafskrivninger
			</h3>
			{data.writeOffs.length === 0 ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen straksafskrivninger endnu. Brug "Straksafskriv" når en
						småanskaffelse er under det skattemæssige minimum og bogføres
						direkte som udgift.
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
						aria-label="Straksafskrivninger"
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
									Navn
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Kategori
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Anskaffet
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Bogført
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition6,
									)}
								>
									Beløb
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Konto
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}
								>
									Hjemmel
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
							{data.writeOffs.map((row) => (
								<WriteOffRow key={row.id} row={row} currency={currency} />
							))}
						</tbody>
					</table>
				</div>
			)}

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.statementCheck,
					cockpitStyles.statementCheckOk,
				)}
			>
				Alle afskrivninger bogføres via den deterministiske kerne (linje: debit
				afskrivnings-udgift, kredit akkumuleret afskrivning) — det er samme kode
				CLI'en og MCP-kald bruger, så posteringerne er identiske på tværs af
				kanaler.
			</p>

			{registerOpen ? (
				<RegisterAssetModal
					slug={slug}
					onClose={() => setRegisterOpen(false)}
					onRefresh={state.reload}
					onCreated={() => {
						setRegisterOpen(false);
						setActionError(null);
						state.reload();
					}}
					onError={setActionError}
				/>
			) : null}

			{writeOffOpen ? (
				<WriteOffModal
					slug={slug}
					onClose={() => setWriteOffOpen(false)}
					onRefresh={state.reload}
					onCreated={() => {
						setWriteOffOpen(false);
						setActionError(null);
						state.reload();
					}}
					onError={setActionError}
				/>
			) : null}
		</section>
	);
}

function AssetRowView({
	row,
	slug,
	currency,
	onPosted,
	onError,
}: {
	row: AssetRow;
	slug: string;
	currency: string;
	onPosted: () => void;
	onError: (msg: string) => void;
}) {
	const [busy, setBusy] = useState(false);
	const [pending, setPending] = useState(false);
	const canDepreciate = row.status === "active" && row.remainingPeriods > 0;

	async function doDepreciate() {
		setBusy(true);
		try {
			await api.depreciateAsset(slug, row.assetId, {
				// Use the LOCAL date — `toISOString()` is UTC and would mis-date a
				// BOOKED afskrivning in Danish evening hours (UTC+1/+2 → off by one).
				transactionDate: todayIso(),
			});
			onPosted();
		} catch (err) {
			onError(
				err instanceof ApiError
					? err.message
					: "Kunne ikke bogføre afskrivningen.",
			);
			throw err;
		} finally {
			setBusy(false);
		}
	}

	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.name}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.category}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.entryDate,
				)}
			>
				{row.acquisitionDate}
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
				{formatKroner(row.cost, currency)}
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
				{formatKroner(row.accumulatedDepreciation, currency)}
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
				{formatKroner(row.netBookValue, currency)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.flag,
						row.status === "active" && cockpitStyles.flagOk,
						!(row.status === "active") && cockpitStyles.flagNeutral,
					)}
				>
					{row.status === "active"
						? `${row.postedPeriods}/${row.usefulLifeMonths} afskrevet`
						: "Fuldt afskrevet"}
				</span>
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<Button
					requiredPermission="company.ledger.post"
					variant="secondary"
					type="button"
					onClick={() => setPending(true)}
					disabled={!canDepreciate || busy}
					aria-label={`Beregn afskrivning for ${row.name}`}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					{busy ? "Bogfører…" : "Beregn afskrivning"}
				</Button>
				{pending && (
					<ConfirmDialog
						title={`Bogfør afskrivning: ${row.name}`}
						body={
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Bogfører næste afskrivningsperiode for anlægget. Posteringen kan
								ikke fortrydes — kun rettes ved at lave en modpostering
								bagefter.
							</p>
						}
						confirmLabel="Bogfør afskrivning"
						confirmKind="danger"
						onConfirm={async () => {
							await doDepreciate();
						}}
						onClose={() => setPending(false)}
						onRefresh={onPosted}
					/>
				)}
			</td>
		</tr>
	);
}

function WriteOffRow({
	row,
	currency,
}: {
	row: AssetWriteOffRow;
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
				{row.name}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.category}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.entryDate,
				)}
			>
				{row.acquisitionDate}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.entryDate,
				)}
			>
				{row.writeOffDate}
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
				{formatKroner(row.cost, currency)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.expenseAccountNo}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.thresholdRuleSource}
			</td>
		</tr>
	);
}

function useDocumentPicker(slug: string) {
	return useAsync<CompanyDocuments>(
		(signal) => api.documents(slug, { signal }),
		[slug],
	);
}

function RegisterAssetModal({
	slug,
	onClose: onDismiss,
	onRefresh,
	onCreated,
	onError,
}: {
	slug: string;
	onClose: () => void;
	onRefresh: () => void;
	onCreated: () => void;
	onError: (msg: string) => void;
}) {
	const docs = useDocumentPicker(slug);
	const [name, setName] = useState("");
	const [category, setCategory] = useState("hardware");
	// Default to the LOCAL date — `toISOString()` is UTC and is off-by-one in
	// Danish evening hours (UTC+1/+2), pre-filling tomorrow's date.
	const [acquisitionDate, setAcquisitionDate] = useState(todayIso());
	const [cost, setCost] = useState("");
	const [usefulLifeMonths, setUsefulLifeMonths] = useState("36");
	const [purchaseDocumentId, setPurchaseDocumentId] = useState<string>("");
	const [note, setNote] = useState("");
	const [busy, setBusy] = useState(false);

	const outcome = useMutationOutcome(onRefresh);
	const guard = useDiscardGuard(
		Boolean(name || cost || purchaseDocumentId || note) ||
			category !== "hardware" ||
			acquisitionDate !== todayIso() ||
			usefulLifeMonths !== "36",
		onDismiss,
	);
	const { onClose } = guard;

	async function handleSubmit(event: React.FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		setBusy(true);
		try {
			const costNumber = parseDanishAmount(cost);
			const months = Number(usefulLifeMonths);
			const docId = Number(purchaseDocumentId);
			if (costNumber === null || costNumber <= 0) {
				throw new Error("Kostprisen skal være et positivt tal.");
			}
			if (!Number.isInteger(months) || months <= 0) {
				throw new Error("Levetiden (måneder) skal være et positivt heltal.");
			}
			if (!Number.isInteger(docId) || docId <= 0) {
				throw new Error("Vælg et bilag som købsbilag.");
			}
			await outcome.run(() =>
				api.registerAsset(slug, {
					name: name.trim(),
					category: category.trim(),
					acquisitionDate,
					cost: costNumber,
					usefulLifeMonths: months,
					purchaseDocumentId: docId,
					...(note.trim() ? { note: note.trim() } : {}),
				}),
			);
			guard.dismiss();
			onCreated();
		} catch (err) {
			onError(
				err instanceof ApiError
					? err.message
					: err instanceof Error
						? err.message
						: "Kunne ikke registrere anlæg.",
			);
		} finally {
			setBusy(false);
		}
	}

	const docRows: DocumentRow[] = docs.data?.documents ?? [];

	return (
		<Dialog
			title="Registrér nyt anlæg"
			onClose={onClose}
			busy={busy}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}

			<form
				onSubmit={handleSubmit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Navn
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						required
						value={name}
						onChange={(e) => setName(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kategori
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						required
						value={category}
						onChange={(e) => setCategory(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Anskaffet (YYYY-MM-DD)
					</span>
					<Input
						disabled={outcome.blocked}
						type="date"
						required
						value={acquisitionDate}
						onChange={(e) => setAcquisitionDate(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kostpris (kr.)
					</span>
					<MoneyInput
						disabled={outcome.blocked}
						required
						value={cost}
						onValueChange={(e) => setCost(e)}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Levetid (måneder, lineær afskrivning)
					</span>
					<Input
						disabled={outcome.blocked}
						type="number"
						step="1"
						min="1"
						required
						value={usefulLifeMonths}
						onChange={(e) => setUsefulLifeMonths(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bilag (købsdokument)
					</span>
					<Select
						disabled={outcome.blocked}
						required
						value={purchaseDocumentId}
						onChange={(e) => setPurchaseDocumentId(e.target.value)}
						xstyle={[cockpitStyles.selectComposition]}
					>
						<option
							value=""
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Vælg bilag…
						</option>
						{docRows.map((d) => (
							<option
								key={d.id}
								value={d.id}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								#{d.id} · {d.documentNo ?? "uden nr."} ·{" "}
								{d.supplierName ?? "ukendt leverandør"}
							</option>
						))}
					</Select>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Note (valgfri)
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						value={note}
						onChange={(e) => setNote(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
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
						requiredPermission="company.ledger.post"
						type="submit"
						disabled={outcome.blocked || busy}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{busy ? "Opretter…" : "Registrér anlæg"}
					</Button>
					<Button
						variant="secondary"
						type="button"
						onClick={onClose}
						disabled={busy}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Annullér
					</Button>
				</div>
			</form>
		</Dialog>
	);
}

function WriteOffModal({
	slug,
	onClose: onDismiss,
	onRefresh,
	onCreated,
	onError,
}: {
	slug: string;
	onClose: () => void;
	onRefresh: () => void;
	onCreated: () => void;
	onError: (msg: string) => void;
}) {
	const docs = useDocumentPicker(slug);
	const [name, setName] = useState("");
	const [category, setCategory] = useState("smaaanskaffelser");
	// Default to the LOCAL date — `toISOString()` is UTC and is off-by-one in
	// Danish evening hours (UTC+1/+2), pre-filling tomorrow's date.
	const [acquisitionDate, setAcquisitionDate] = useState(todayIso());
	const [transactionDate, setTransactionDate] = useState(todayIso());
	const [cost, setCost] = useState("");
	const [purchaseDocumentId, setPurchaseDocumentId] = useState<string>("");
	const [expenseAccountNo, setExpenseAccountNo] = useState(
		DEFAULT_EXPENSE_ACCOUNT,
	);
	const [thresholdRuleSource, setThresholdRuleSource] = useState(
		DEFAULT_THRESHOLD_RULE,
	);
	const [note, setNote] = useState("");
	const [busy, setBusy] = useState(false);

	const outcome = useMutationOutcome(onRefresh);
	const guard = useDiscardGuard(
		Boolean(name || cost || purchaseDocumentId || note) ||
			category !== "smaaanskaffelser" ||
			acquisitionDate !== todayIso() ||
			transactionDate !== todayIso() ||
			expenseAccountNo !== DEFAULT_EXPENSE_ACCOUNT ||
			thresholdRuleSource !== DEFAULT_THRESHOLD_RULE,
		onDismiss,
	);
	const { onClose } = guard;

	async function handleSubmit(event: React.FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		setBusy(true);
		try {
			const costNumber = parseDanishAmount(cost);
			const docId = Number(purchaseDocumentId);
			if (costNumber === null || costNumber <= 0) {
				throw new Error("Beløbet skal være positivt.");
			}
			if (!Number.isInteger(docId) || docId <= 0) {
				throw new Error("Vælg et bilag som købsbilag.");
			}
			if (!thresholdRuleSource.trim()) {
				throw new Error(
					"Hjemmelshenvisningen (tærskelregel) er obligatorisk for straksafskrivning.",
				);
			}
			await outcome.run(() =>
				api.writeOffAsset(slug, {
					name: name.trim(),
					category: category.trim(),
					acquisitionDate,
					transactionDate,
					cost: costNumber,
					purchaseDocumentId: docId,
					expenseAccountNo: expenseAccountNo.trim(),
					thresholdRuleSource: thresholdRuleSource.trim(),
					...(note.trim() ? { note: note.trim() } : {}),
				}),
			);
			guard.dismiss();
			onCreated();
		} catch (err) {
			onError(
				err instanceof ApiError
					? err.message
					: err instanceof Error
						? err.message
						: "Kunne ikke straksafskrive.",
			);
		} finally {
			setBusy(false);
		}
	}

	const docRows: DocumentRow[] = docs.data?.documents ?? [];

	return (
		<Dialog
			title="Straksafskriv småanskaffelse"
			onClose={onClose}
			busy={busy}
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
				Straksafskrivning er en skattemæssig vurdering — du bekræfter med en
				eksplicit hjemmelshenvisning, og handlingen bogføres som en udgift
				direkte. Bilag og hjemmel arkiveres på audit-sporet.
			</p>
			<form
				onSubmit={handleSubmit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Navn
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						required
						value={name}
						onChange={(e) => setName(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kategori
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						required
						value={category}
						onChange={(e) => setCategory(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Anskaffet (YYYY-MM-DD)
					</span>
					<Input
						disabled={outcome.blocked}
						type="date"
						required
						value={acquisitionDate}
						onChange={(e) => setAcquisitionDate(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bogføringsdato
					</span>
					<Input
						disabled={outcome.blocked}
						type="date"
						required
						value={transactionDate}
						onChange={(e) => setTransactionDate(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Beløb (kr.)
					</span>
					<MoneyInput
						disabled={outcome.blocked}
						required
						value={cost}
						onValueChange={(e) => setCost(e)}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bilag (købsdokument)
					</span>
					<Select
						disabled={outcome.blocked}
						required
						value={purchaseDocumentId}
						onChange={(e) => setPurchaseDocumentId(e.target.value)}
						xstyle={[cockpitStyles.selectComposition]}
					>
						<option
							value=""
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Vælg bilag…
						</option>
						{docRows.map((d) => (
							<option
								key={d.id}
								value={d.id}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								#{d.id} · {d.documentNo ?? "uden nr."} ·{" "}
								{d.supplierName ?? "ukendt leverandør"}
							</option>
						))}
					</Select>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Udgiftskonto
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						required
						value={expenseAccountNo}
						onChange={(e) => setExpenseAccountNo(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Hjemmelshenvisning (tærskelregel)
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						required
						value={thresholdRuleSource}
						onChange={(e) => setThresholdRuleSource(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Note (valgfri)
					</span>
					<Input
						disabled={outcome.blocked}
						type="text"
						value={note}
						onChange={(e) => setNote(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
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
						requiredPermission="company.ledger.post"
						type="submit"
						disabled={outcome.blocked || busy}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{busy ? "Bogfører…" : "Straksafskriv"}
					</Button>
					<Button
						variant="secondary"
						type="button"
						onClick={onClose}
						disabled={busy}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Annullér
					</Button>
				</div>
			</form>
		</Dialog>
	);
}

const viewStyles = stylex.create({
	site0: { marginTop: "1rem" },
	site1: { marginTop: "1.5rem" },
	site2: { marginTop: "1.5rem" },
});
