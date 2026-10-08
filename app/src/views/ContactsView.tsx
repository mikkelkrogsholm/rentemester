import * as stylex from "@stylexjs/stylex";
import {
	Button,
	ButtonLink,
	FilterBar,
	Input,
	PageHeader,
	Pagination,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Kontakter — the per-company customers and vendors (cockpit-redesign it. 5).
//
// Renders `/api/companies/:slug/contacts`: the master data — customers (kunder)
// and vendors (leverandører) — each in its own table with the key figures the
// ledger keys off (CVR, betalingsbetingelser, standardkonto). Contacts are not
// year-scoped, but the company sub-nav still carries the selected `?year=` so
// it follows the user across views — the fiscal years for the selector are
// fetched from the response. A company with no contacts shows a graceful
// empty state.
//
// #390: the page is now ALSO the daily-maintenance surface. The page-head
// exposes a primary "Tilføj kunde" + "Tilføj leverandør" action; each row in
// either table is clickable and opens the same modal in edit-mode. The
// Importér button remains for one-off CSV migrations.

import { useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import {
	ContactFormModal,
	type ContactKind,
} from "../components/ContactFormModal";
import { ErrorState, Loading } from "../components/Feedback";
import { ImportModal } from "../components/ImportModal";
import { PartyLink } from "../components/PartyLink";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type {
	CompanyContacts,
	ContactCustomerRow,
	ContactVendorRow,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

/** VAT-treatment codes from the ledger, mapped to a Danish label. */
const VAT_TREATMENT_LABELS: Record<string, string> = {
	standard: "Standardmoms",
	domestic_reverse_charge: "Omvendt betalingspligt (DK)",
	foreign_reverse_charge: "Omvendt betalingspligt (EU-tjenester)",
	exempt: "Momsfritaget",
};

/** Local UI state when the create/edit modal is open. */
type ModalState =
	| { kind: "customer"; row?: ContactCustomerRow }
	| { kind: "vendor"; row?: ContactVendorRow };

/**
 * #430 — pending delete-bekræftelse. Når den er sat, viser cockpittet en
 * `ConfirmDialog` med en menneske-læselig beskrivelse af konsekvenserne;
 * `onConfirm` kalder `api.deleteCustomer` / `api.deleteVendor` som server-
 * side blokerer hvis kontakten er i brug på en åben faktura/gæld.
 */
type DeleteState =
	| { kind: "customer"; row: ContactCustomerRow }
	| { kind: "vendor"; row: ContactVendorRow };

export function ContactsView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const state = useAsync<CompanyContacts>(
		(signal) => api.contacts(slug, { signal }),
		[slug],
	);
	const [params, setParams] = useSearchParams();
	const query = params.get("q") ?? "";
	const kindFilter = params.get("kind") ?? "all";
	function setContactFilter(key: "q" | "kind", value: string) {
		const next = new URLSearchParams(params);
		if (value && value !== "all") next.set(key, value);
		else next.delete(key);
		next.delete("page");
		setParams(next, { replace: true });
	}
	function changePage(value: number, size: number) {
		const next = new URLSearchParams(params);
		next.set("page", String(value));
		next.set("pageSize", String(size));
		setParams(next, { replace: true });
	}
	// True while the generic file-import modal is open.
	const [importing, setImporting] = useState(false);
	// The create/edit modal — undefined when closed.
	const [modal, setModal] = useState<ModalState | undefined>(undefined);
	// #430 — pending delete-bekræftelse (kunde eller leverandør). Undefined når
	// ingen dialog er åben.
	const [pendingDelete, setPendingDelete] = useState<DeleteState | undefined>(
		undefined,
	);

	if (state.loading && !state.data)
		return <Loading label="Henter kontakter…" />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const c = state.data!;
	const currency = c.company.currency || "DKK";
	const selectedYear =
		year ??
		c.fiscalYears.find((y) => y.source === "live")?.label ??
		c.fiscalYears[0]?.label ??
		String(new Date().getFullYear());
	const total = c.customers.length + c.vendors.length;
	const needle = query.trim().toLocaleLowerCase("da");
	const matches = (row: {
		name: string;
		vatOrCvr: string | null;
		email?: string | null;
	}) =>
		!needle ||
		[row.name, row.vatOrCvr, row.email].some((value) =>
			value?.toLocaleLowerCase("da").includes(needle),
		);
	const customers =
		kindFilter === "vendors"
			? []
			: c.customers
					.filter(matches)
					.sort((a, b) => a.name.localeCompare(b.name, "da"));
	const vendors =
		kindFilter === "customers"
			? []
			: c.vendors
					.filter(matches)
					.sort((a, b) => a.name.localeCompare(b.name, "da"));
	const filteredTotal = customers.length + vendors.length;
	const requestedSize = Number(params.get("pageSize"));
	const pageSize = [25, 50, 100].includes(requestedSize) ? requestedSize : 50;
	const requestedPage = Number(params.get("page"));
	const page = Math.min(
		Math.max(1, Number.isSafeInteger(requestedPage) ? requestedPage : 1),
		Math.max(1, Math.ceil(filteredTotal / pageSize)),
	);
	const first = (page - 1) * pageSize;
	const pageCustomers = customers.slice(first, first + pageSize);
	const pageVendors = vendors.slice(
		Math.max(0, first - customers.length),
		Math.max(0, first + pageSize - customers.length),
	);

	function openCreate(kind: ContactKind) {
		setModal({ kind } as ModalState);
	}

	function openEditCustomer(row: ContactCustomerRow) {
		setModal({ kind: "customer", row });
	}

	function openEditVendor(row: ContactVendorRow) {
		setModal({ kind: "vendor", row });
	}

	function openDeleteCustomer(row: ContactCustomerRow) {
		setPendingDelete({ kind: "customer", row });
	}

	function openDeleteVendor(row: ContactVendorRow) {
		setPendingDelete({ kind: "vendor", row });
	}

	return (
		<section
			data-cockpit-page="contacts"
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
				title="Kontakter"
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
								requiredPermission="company.master-data"
								type="button"
								onClick={() => openCreate("customer")}
								xstyle={[cockpitStyles.statementBtnComposition]}
							>
								Tilføj kunde
							</Button>
							<Button
								requiredPermission="company.master-data"
								type="button"
								onClick={() => openCreate("vendor")}
								xstyle={[cockpitStyles.statementBtnComposition]}
							>
								Tilføj leverandør
							</Button>
							<Button
								requiredPermission="company.ledger.post"
								variant="secondary"
								type="button"
								onClick={() => setImporting(true)}
								xstyle={[cockpitStyles.statementBtnComposition]}
							>
								Importér
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
						{c.company.cvr ? `CVR ${c.company.cvr} · ` : ""}
						{c.company.country} · {currency} · Kontakter
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={c.fiscalYears}
				selectedYear={selectedYear}
				onYearChange={setYear}
			/>

			{importing && (
				<ImportModal
					slug={slug}
					onImported={state.reload}
					onClose={() => setImporting(false)}
				/>
			)}

			{modal && (
				<ContactFormModal
					slug={slug}
					kind={modal.kind}
					customer={modal.kind === "customer" ? modal.row : undefined}
					vendor={modal.kind === "vendor" ? modal.row : undefined}
					onSaved={state.reload}
					onClose={() => setModal(undefined)}
				/>
			)}

			{pendingDelete && (
				<ConfirmDialog
					title={
						pendingDelete.kind === "customer"
							? `Slet kunde ${pendingDelete.row.name}?`
							: `Slet leverandør ${pendingDelete.row.name}?`
					}
					body={
						<>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{pendingDelete.kind === "customer"
									? "Kunden fjernes fra dine fremtidige fakturaer og dropdowns."
									: "Leverandøren fjernes fra dine fremtidige bilag og dropdowns."}
							</p>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Allerede bogførte fakturaer og posteringer beholder navnet som
								det var på bogføringstidspunktet — historikken og
								revisor-eksporten er ikke påvirket.
							</p>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								{pendingDelete.kind === "customer"
									? "Hvis kunden er i brug på en åben (ikke-betalt) faktura, bliver sletningen blokeret med et henvisning til fakturanummeret."
									: "Hvis leverandøren har en åben gæld der ikke er betalt endnu, bliver sletningen blokeret med en henvisning til regningen."}
							</p>
						</>
					}
					confirmLabel="Slet"
					confirmKind="danger"
					onConfirm={async () => {
						if (pendingDelete.kind === "customer") {
							await api.deleteCustomer(slug, pendingDelete.row.id);
						} else {
							await api.deleteVendor(slug, pendingDelete.row.id);
						}
						// Reload so the deleted row disappears immediately.
						state.reload();
					}}
					onClose={() => setPendingDelete(undefined)}
					onRefresh={state.reload}
				/>
			)}

			<FilterBar
				activeCount={Number(Boolean(query)) + Number(kindFilter !== "all")}
				onReset={() => {
					const next = new URLSearchParams(params);
					next.delete("q");
					next.delete("kind");
					next.delete("page");
					setParams(next, { replace: true });
				}}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Søg kontakter
					<Input
						type="search"
						value={query}
						onChange={(event) => setContactFilter("q", event.target.value)}
						placeholder="Navn, CVR eller e-mail"
						xstyle={[cockpitStyles.statementInputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Kontakttype
					<Select
						value={kindFilter}
						onChange={(event) => setContactFilter("kind", event.target.value)}
						xstyle={[cockpitStyles.statementSelectComposition]}
					>
						<option
							value="all"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Alle kontakter
						</option>
						<option
							value="customers"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Kunder
						</option>
						<option
							value="vendors"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Leverandører
						</option>
					</Select>
				</label>
			</FilterBar>
			{total > 0 && filteredTotal === 0 && (
				<p
					role="status"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Ingen kontakter matcher filtrene.
				</p>
			)}
			{total === 0 ? (
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
						Ingen kontakter endnu
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Der er ingen registrerede kunder eller leverandører for denne
						virksomhed. Brug «Tilføj kunde» eller «Tilføj leverandør» ovenfor
						for at oprette stamdata — eller «Importér» til at hente kontakter
						fra et tidligere bogføringssystem.
					</p>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rowActions,
							viewStyles.site0,
						)}
					>
						<Button
							requiredPermission="company.master-data"
							type="button"
							onClick={() => openCreate("customer")}
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Tilføj kunde
						</Button>
						<Button
							requiredPermission="company.master-data"
							type="button"
							onClick={() => openCreate("vendor")}
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Tilføj leverandør
						</Button>
					</div>
				</div>
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
						{c.customers.length} {c.customers.length === 1 ? "kunde" : "kunder"}{" "}
						· {c.vendors.length}{" "}
						{c.vendors.length === 1 ? "leverandør" : "leverandører"}
					</p>

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
							Kunder
						</h3>
						{pageCustomers.length === 0 && customers.length > 0 ? (
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Kunderne vises på en anden side.
							</p>
						) : (
							<CustomerTable
								customers={pageCustomers}
								slug={slug}
								onEdit={openEditCustomer}
								onDelete={openDeleteCustomer}
							/>
						)}
					</div>

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
							Leverandører
						</h3>
						{pageVendors.length === 0 && vendors.length > 0 ? (
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Leverandørerne vises på en anden side.
							</p>
						) : (
							<VendorTable
								vendors={pageVendors}
								slug={slug}
								onEdit={openEditVendor}
								onDelete={openDeleteVendor}
							/>
						)}
					</div>
				</>
			)}
			{total > 0 && (
				<Pagination
					total={filteredTotal}
					page={page}
					pageSize={pageSize}
					onPageChange={(value) => changePage(value, pageSize)}
					onPageSizeChange={(size) => changePage(1, size)}
				/>
			)}
		</section>
	);
}

function CustomerTable({
	customers,
	slug,
	onEdit,
	onDelete,
}: {
	customers: ContactCustomerRow[];
	slug: string;
	onEdit: (row: ContactCustomerRow) => void;
	onDelete: (row: ContactCustomerRow) => void;
}) {
	return (
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
				role="table"
				aria-label="Kunder"
				{...stylex.props(cockpitStyles.dailyListTableComposition)}
			>
				<thead {...stylex.props(cockpitStyles.dailyListTheadComposition)}>
					<tr
						role="row"
						{...stylex.props(cockpitStyles.dailyListTrComposition)}
					>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							Navn
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							CVR / moms-nr.
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							E-mail
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							Valuta
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition2)}
						>
							Betalingsfrist
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition2)}
						>
							Udestående
						</th>
						<th
							aria-label="Handlinger"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						/>
					</tr>
				</thead>
				<tbody {...stylex.props(cockpitStyles.dailyListTbodyComposition)}>
					{customers.length === 0 ? (
						<tr
							role="row"
							{...stylex.props(cockpitStyles.dailyListTrComposition)}
						>
							<td
								role="cell"
								colSpan={7}
								{...stylex.props(cockpitStyles.dailyListTdComposition)}
							>
								Ingen kunder registreret.
							</td>
						</tr>
					) : (
						customers.map((row) => (
							<tr
								role="row"
								key={row.id}
								{...stylex.props(cockpitStyles.dailyListTrComposition)}
							>
								<td
									data-label="Navn"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition2)}
								>
									<PartyLink slug={slug} partyId={row.partyId}>
										{row.name}
									</PartyLink>
								</td>
								<td
									data-label="CVR / moms-nr."
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition3)}
								>
									{row.vatOrCvr ?? "—"}
								</td>
								<td
									data-label="E-mail"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition2)}
								>
									{row.email ?? "—"}
								</td>
								<td
									data-label="Valuta"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition2)}
								>
									{row.defaultCurrency}
								</td>
								<td
									data-label="Betalingsfrist"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition4)}
								>
									{row.paymentTermsDays} dage
								</td>
								<td
									data-label="Udestående"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition4)}
								>
									{row.openInvoiceCount === 0 ? (
										<span
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											—
										</span>
									) : (
										<>
											<span
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{formatKroner(row.openBalance, row.defaultCurrency)}
											</span>
											{row.overdueCount > 0 && (
												<>
													{" "}
													<span
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
															cockpitStyles.flag,
															cockpitStyles.flagCritical,
														)}
													>
														{row.overdueCount} forfalden
														{row.overdueCount === 1 ? "" : "e"}
													</span>
												</>
											)}
										</>
									)}
								</td>
								<td
									data-label="Oplysning"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition5)}
								>
									<Button
										requiredPermission="company.master-data"
										variant="secondary"
										type="button"
										onClick={() => onEdit(row)}
										aria-label={`Redigér ${row.name}`}
										xstyle={[cockpitStyles.buttonComposition]}
									>
										Redigér
									</Button>
									<Button
										requiredPermission="company.master-data"
										variant="danger"
										type="button"
										onClick={() => onDelete(row)}
										aria-label={`Slet ${row.name}`}
										xstyle={[cockpitStyles.buttonComposition]}
									>
										Slet
									</Button>
								</td>
							</tr>
						))
					)}
				</tbody>
			</table>
		</div>
	);
}

function VendorTable({
	vendors,
	slug,
	onEdit,
	onDelete,
}: {
	vendors: ContactVendorRow[];
	slug: string;
	onEdit: (row: ContactVendorRow) => void;
	onDelete: (row: ContactVendorRow) => void;
}) {
	return (
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
				role="table"
				aria-label="Leverandører"
				{...stylex.props(cockpitStyles.dailyListTableComposition)}
			>
				<thead {...stylex.props(cockpitStyles.dailyListTheadComposition)}>
					<tr
						role="row"
						{...stylex.props(cockpitStyles.dailyListTrComposition)}
					>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							Navn
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							CVR / moms-nr.
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							Standard udgiftskonto
						</th>
						<th
							role="columnheader"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						>
							Momsbehandling
						</th>
						<th
							aria-label="Handlinger"
							{...stylex.props(cockpitStyles.tableStatementTableThComposition)}
						/>
					</tr>
				</thead>
				<tbody {...stylex.props(cockpitStyles.dailyListTbodyComposition)}>
					{vendors.length === 0 ? (
						<tr
							role="row"
							{...stylex.props(cockpitStyles.dailyListTrComposition)}
						>
							<td
								role="cell"
								colSpan={5}
								{...stylex.props(cockpitStyles.dailyListTdComposition)}
							>
								Ingen leverandører registreret.
							</td>
						</tr>
					) : (
						vendors.map((row) => (
							<tr
								role="row"
								key={row.id}
								{...stylex.props(cockpitStyles.dailyListTrComposition)}
							>
								<td
									data-label="Navn"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition2)}
								>
									<PartyLink slug={slug} partyId={row.partyId}>
										{row.name}
									</PartyLink>
								</td>
								<td
									data-label="CVR / moms-nr."
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition3)}
								>
									{row.vatOrCvr ?? "—"}
								</td>
								<td
									data-label="Standard udgiftskonto"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition3)}
								>
									{row.defaultExpenseAccount ?? "—"}
								</td>
								<td
									data-label="Momsbehandling"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition2)}
								>
									{row.defaultVatTreatment
										? (VAT_TREATMENT_LABELS[row.defaultVatTreatment] ??
											row.defaultVatTreatment)
										: "—"}
								</td>
								<td
									data-label="Oplysning"
									role="cell"
									{...stylex.props(cockpitStyles.dailyListTdComposition5)}
								>
									<Button
										requiredPermission="company.master-data"
										variant="secondary"
										type="button"
										onClick={() => onEdit(row)}
										aria-label={`Redigér ${row.name}`}
										xstyle={[cockpitStyles.buttonComposition]}
									>
										Redigér
									</Button>
									<Button
										requiredPermission="company.master-data"
										variant="danger"
										type="button"
										onClick={() => onDelete(row)}
										aria-label={`Slet ${row.name}`}
										xstyle={[cockpitStyles.buttonComposition]}
									>
										Slet
									</Button>
								</td>
							</tr>
						))
					)}
				</tbody>
			</table>
		</div>
	);
}

const viewStyles = stylex.create({
	site0: { marginTop: "1rem" },
});
