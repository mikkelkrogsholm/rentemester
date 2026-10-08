import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Kontoplan view (#344) — read-only per-virksomhed liste over alle konti
// (nummer, navn, type, normal-saldo, evt. moms-mapping) med søg + filter pr.
// type, og en kort summary pr. type. Genbruger eksisterende
// /api/companies/:slug/accounts uden duplikeret core-logik.
//
// Note: åbningsbalance-flowet (CSV-import / pr. konto-indtastning) er ikke
// inkluderet i denne PR — det er et write-flow med actor + audit-event og
// følger som follow-up. Read-side er nu fuld.

import { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
	FilterBar,
	PageState,
	ResponsiveTable,
} from "../components/CockpitPrimitives";
import { api } from "../lib/api";
import type {
	AccountRole,
	AccountRoleResolution,
	AccountRow,
	CompanyAccounts,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";

const TYPE_LABELS: Record<string, string> = {
	asset: "Aktiv",
	liability: "Passiv",
	equity: "Egenkapital",
	income: "Indtægt",
	expense: "Omkostning",
	vat: "Moms",
};

const ACCOUNT_ROLE_LABELS: Record<AccountRole, string> = {
	bank: "Bankkonto",
	debtors: "Debitorer",
	creditors: "Kreditorer",
	output_vat: "Salgsmoms",
	input_vat: "Købsmoms",
	reverse_charge_vat: "Omvendt betalingspligt-moms",
	vat_settlement: "Momsafregning",
	operational_default: "Standard driftskonto",
};

const ACCOUNT_ROLE_STATUS_LABELS = {
	complete: "Komplet",
	incomplete: "Ufuldstændig",
	ambiguous: "Kræver menneskelig afklaring",
} as const;

export function AccountsView() {
	const { slug = "" } = useParams();
	const state = useAsync<CompanyAccounts>(
		(signal) => api.accounts(slug, { signal }),
		[slug],
	);
	const [typeFilter, setTypeFilter] = useState<string>("");
	const [search, setSearch] = useState("");

	const filtered = useMemo(() => {
		const needle = search.trim().toLowerCase();
		return (state.data?.accounts ?? []).filter((a) => {
			if (typeFilter !== "" && a.type !== typeFilter) return false;
			if (needle === "") return true;
			return (
				a.accountNo.toLowerCase().includes(needle) ||
				a.name.toLowerCase().includes(needle) ||
				(a.defaultVatCode ?? "").toLowerCase().includes(needle)
			);
		});
	}, [state.data, typeFilter, search]);

	if (state.loading)
		return <PageState kind="loading" title="Henter kontoplan" />;
	if (state.error)
		return (
			<PageState
				kind="error"
				title="Kontoplan kunne ikke hentes"
				onRetry={state.reload}
			>
				{state.error}
			</PageState>
		);
	const data = state.data!;

	return (
		<section
			data-cockpit-page="accounts"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Kontoplan"
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
								xstyle={[cockpitStyles.accountsViewBtnComposition]}
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
						{data.company.country} · {data.company.currency} · Kontoplan
					</p>
				</div>
			</PageHeader>

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Kontoplanen hjælper dig med at vælge den rigtige konto, når du bogfører.
			</p>

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
					Gennemgå kontoplan
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{data.accounts.length} konti er klar til at blive søgt og filtreret.
				</p>
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
					Sammentælling pr. type
				</h3>
				<FilterBar
					activeFilters={
						typeFilter ? [`Type: ${TYPE_LABELS[typeFilter] ?? typeFilter}`] : []
					}
					onReset={() => setTypeFilter("")}
				>
					{Object.entries(data.byType)
						.sort((a, b) => a[0].localeCompare(b[0]))
						.map(([type, count]) => (
							<Button
								key={type}
								type="button"
								onClick={() => setTypeFilter(typeFilter === type ? "" : type)}
								variant={!(typeFilter === type) ? "secondary" : "primary"}
								xstyle={[cockpitStyles.accountsViewBtnComposition2]}
							>
								{TYPE_LABELS[type] ?? type}: {count}
							</Button>
						))}
				</FilterBar>
			</section>

			<details
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Avanceret: kontoroller og importgrundlag
				</summary>
				<AccountRolesCard accountRoles={data.accountRoles} />
			</details>

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
					Kontoplan ({filtered.length} af {data.accounts.length})
				</h3>
				<FilterBar
					activeFilters={search ? [`Søgning: ${search}`] : []}
					onReset={() => setSearch("")}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Søg{" "}
						<Input
							type="search"
							value={search}
							onChange={(e) => setSearch(e.target.value)}
							placeholder="kontonummer, navn, vat-kode …"
							xstyle={[cockpitStyles.accountsViewInputComposition]}
						/>
					</label>
				</FilterBar>

				<ResponsiveTable
					label="Kontoplan"
					xstyle={[cockpitStyles.tableDataComposition]}
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
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Kontonr.
							</th>
							<th
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Navn
							</th>
							<th
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Type
							</th>
							<th
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Normal saldo
							</th>
							<th
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Default moms-kode
							</th>
							<th
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Bogføringslinjer
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
						{filtered.map((a) => (
							<AccountListRow key={a.accountNo} row={a} />
						))}
						{filtered.length === 0 && (
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<td
									colSpan={6}
									{...stylex.props(cockpitStyles.tableDataTdComposition)}
								>
									Ingen konti matcher filtret.
								</td>
							</tr>
						)}
					</tbody>
				</ResponsiveTable>
			</section>
		</section>
	);
}

function AccountRolesCard({
	accountRoles,
}: Pick<CompanyAccounts, "accountRoles">) {
	const needsHumanResolution = accountRoles.status !== "complete";
	return (
		<section
			aria-labelledby="account-role-heading"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
			)}
		>
			<h3
				id="account-role-heading"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
				)}
			>
				Kontoroller
			</h3>
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				Status: {ACCOUNT_ROLE_STATUS_LABELS[accountRoles.status]}
			</p>
			{needsHumanResolution && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{accountRoles.status === "ambiguous"
						? "Flere mulige konti skal bekræftes af et menneske, før bogføring fortsætter."
						: "Manglende roller skal bekræftes af et menneske, før bogføring fortsætter."}
				</p>
			)}
			<ResponsiveTable
				label="Kontoroller"
				xstyle={[cockpitStyles.tableDataComposition]}
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
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTh,
								cockpitStyles.responsiveTableThLastChild,
								cockpitStyles.tableDataTh,
							)}
						>
							Rolle
						</th>
						<th
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTh,
								cockpitStyles.responsiveTableThLastChild,
								cockpitStyles.tableDataTh,
							)}
						>
							Dry-run opløsning
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
					{accountRoles.resolutions.map((resolution) => (
						<AccountRoleResolutionRow
							key={resolution.role}
							resolution={resolution}
						/>
					))}
				</tbody>
			</ResponsiveTable>
			{accountRoles.proposals.length > 0 && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Importforslag:{" "}
					{accountRoles.proposals
						.map(
							(proposal) =>
								`${ACCOUNT_ROLE_LABELS[proposal.role]} → ${proposal.accountNo} (${proposal.source})`,
						)
						.join(", ")}
				</p>
			)}
			{accountRoles.reasons.length > 0 && (
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{accountRoles.reasons
						.map(
							(reason) =>
								`${ACCOUNT_ROLE_LABELS[reason.role]}: ${reason.reason}`,
						)
						.join(" · ")}
				</p>
			)}
		</section>
	);
}

function AccountRoleResolutionRow({
	resolution,
}: {
	resolution: AccountRoleResolution;
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
				{ACCOUNT_ROLE_LABELS[resolution.role]}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{resolution.ok ? (
					<>
						Konto{" "}
						<code
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.code,
							)}
						>
							{resolution.accountNo}
						</code>{" "}
						(version {resolution.version})
					</>
				) : (
					<>Kræver menneskelig afklaring: {resolution.error}</>
				)}
			</td>
		</tr>
	);
}

function AccountListRow({ row }: { row: AccountRow }) {
	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
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
					{row.accountNo}
				</code>
			</td>
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
				{TYPE_LABELS[row.type] ?? row.type}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.normalBalance === "debit" ? "Debet" : "Kredit"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{row.defaultVatCode ? (
					<code
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}
					>
						{row.defaultVatCode}
					</code>
				) : (
					"—"
				)}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					!row.hasPostings && cockpitStyles.muted,
				)}
			>
				{row.hasPostings ? "Ja" : "Nej"}
			</td>
		</tr>
	);
}
