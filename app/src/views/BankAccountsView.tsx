import * as stylex from "@stylexjs/stylex";
import { ErrorState, Loading } from "../components/Feedback";
import {
	Button,
	ButtonLink,
	Dialog,
	Input,
	PageHeader,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
// Bankkonti + CSV-mapping-profiler (#345).
//
// Per-virksomhed liste over registrerede bankkonti + de indbyggede
// CSV-mapping-profiler (Lunar, Danske Bank, Sydbank, …). 'Opret konto'-
// modal kalder POST /api/companies/:slug/bank-accounts.
//
// Note: pr.-konto mapping-override + sample-CSV-preview er parkeret som
// follow-up — read-side + create-flow er nu fuld. CSV-import ad hoc
// foregår fortsat via BankImportModal som genbruger profilerne.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { ResponsiveTable } from "../components/CockpitPrimitives";
import { LegacyBankBindingModal } from "../components/LegacyBankBindingModal";
import { ApiError, api } from "../lib/api";
import type { BankAccount, CompanyBankAccounts } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function BankAccountsView() {
	const { slug = "" } = useParams();
	const [openCreate, setOpenCreate] = useState(false);
	const [legacyBinding, setLegacyBinding] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const state = useAsync<CompanyBankAccounts>(
		(signal) => api.bankAccounts(slug, { signal }),
		[slug],
	);

	if (state.loading && !state.data) return <Loading />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;
	const data = state.data!;

	return (
		<section
			data-cockpit-page="bank-accounts"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
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
				title="Bankkonti"
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
								requiredPermission="company.admin"
								type="button"
								onClick={() => {
									setError(null);
									setOpenCreate(true);
								}}
								xstyle={[cockpitStyles.bankAccountsViewBtnComposition]}
							>
								Opret bankkonto …
							</Button>
							<ButtonLink
								to={`/companies/${slug}/manage`}
								variant={"secondary"}
								xstyle={[cockpitStyles.bankAccountsViewBtnComposition2]}
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
						{data.company.country} · Bankkonti
					</p>
				</div>
			</PageHeader>

			{error && (
				<div
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</div>
			)}
			{legacyBinding && (
				<LegacyBankBindingModal
					slug={slug}
					accounts={data.accounts}
					onApplied={state.reload}
					onClose={() => setLegacyBinding(false)}
				/>
			)}

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
					Bankkonti til den daglige bogføring
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Registrér den konto, du bruger til at hente og afstemme
					bankbevægelser.
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
					Registrerede bankkonti ({data.accounts.length})
				</h3>
				{data.accounts.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen bankkonti endnu. Opret én for at kunne importere bank-CSV via{" "}
						<code
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.code,
							)}
						>
							BankImportModal
						</code>{" "}
						eller CLI'ens{" "}
						<code
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.code,
							)}
						>
							bank import
						</code>
						.
					</p>
				) : (
					<ResponsiveTable
						label="Registrerede bankkonti"
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
									Bank
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
									Reg.nr.
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
									Konto-nr.
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
									IBAN
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
									SWIFT/BIC
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
									Kontoejer / kundenr.
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
									Valuta
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
									Ledger-konto
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
							{data.accounts.map((a) => (
								<BankAccountRow key={a.id} account={a} />
							))}
						</tbody>
					</ResponsiveTable>
				)}
			</section>

			<details
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Avanceret: legacy-binding og importprofiler
				</summary>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<Button
						type="button"
						disabled={
							!data.accounts.some((account) => account.ledgerAccountNo === null)
						}
						onClick={() => setLegacyBinding(true)}
						variant={"secondary"}
						xstyle={[cockpitStyles.bankAccountsViewBtnComposition]}
					>
						Bind ældre bankkonto
					</Button>
				</div>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Indbyggede CSV-mapping-profiler ({data.profiles.length})
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					BankImportModal og CLI'ens{" "}
					<code
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}
					>
						bank import --profile &lt;navn&gt;
					</code>
					genbruger disse hard-kodede mapping-profiler. Pr.-konto mapping-
					override er en follow-up.
				</p>
				<ResponsiveTable
					label="CSV-mapping-profiler"
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
								Profil-navn
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
								Bank
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
								Separator
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
								Encoding
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
								Dato-format
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
						{data.profiles.map((p) => (
							<tr
								key={p.name}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
									<code
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}
									>
										{p.name}
									</code>
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
									{p.bankName ?? "—"}
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
									<code
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}
									>
										{p.separator ?? ";"}
									</code>
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
									{p.encoding ?? "utf-8"}
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdComposition2)}>
									{p.dateOrder ?? "dmy"}
								</td>
							</tr>
						))}
					</tbody>
				</ResponsiveTable>
			</details>

			{openCreate && (
				<CreateBankAccountModal
					slug={slug}
					onClose={() => setOpenCreate(false)}
					onRefresh={state.reload}
					onDone={() => {
						setOpenCreate(false);
						state.reload();
					}}
					onError={(msg) => setError(msg)}
				/>
			)}
		</section>
	);
}

function BankAccountRow({ account }: { account: BankAccount }) {
	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.name}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.bankName ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.registrationNo ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.accountNo ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.iban ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.bic ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{[account.accountOwner, account.customerNo]
					.filter(Boolean)
					.join(" / ") || "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.currency}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				{account.ledgerAccountNo ?? "—"}
			</td>
			<td
				{...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}
			>
				<span
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{account.active ? "Aktiv" : "Inaktiv"}
				</span>
			</td>
		</tr>
	);
}

function CreateBankAccountModal({
	slug,
	onClose: onDismiss,
	onRefresh,
	onDone,
	onError,
}: {
	slug: string;
	onClose: () => void;
	onRefresh: () => void;
	onDone: () => void;
	onError: (msg: string) => void;
}) {
	const [name, setName] = useState("");
	const [bankName, setBankName] = useState("");
	const [registrationNo, setRegistrationNo] = useState("");
	const [accountNo, setAccountNo] = useState("");
	const [iban, setIban] = useState("");
	const [bic, setBic] = useState("");
	const [accountOwner, setAccountOwner] = useState("");
	const [customerNo, setCustomerNo] = useState("");
	const [currency, setCurrency] = useState("DKK");
	const [ledgerAccountNo, setLedgerAccountNo] = useState("");
	const [submitting, setSubmitting] = useState(false);

	const outcome = useMutationOutcome(onRefresh);
	const guard = useDiscardGuard(
		Boolean(
			name ||
				bankName ||
				registrationNo ||
				accountNo ||
				iban ||
				bic ||
				accountOwner ||
				customerNo ||
				ledgerAccountNo,
		),
		onDismiss,
	);
	const { onClose } = guard;

	const submit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (outcome.isBlocked()) return;
		setSubmitting(true);
		try {
			await outcome.run(() =>
				api.createBankAccount(slug, {
					name,
					...(bankName ? { bankName } : {}),
					...(registrationNo ? { registrationNo } : {}),
					...(accountNo ? { accountNo } : {}),
					...(iban ? { iban } : {}),
					...(bic ? { bic } : {}),
					...(accountOwner ? { accountOwner } : {}),
					...(customerNo ? { customerNo } : {}),
					...(currency ? { currency } : {}),
					...(ledgerAccountNo ? { ledgerAccountNo } : {}),
				}),
			);
			guard.dismiss();
			onDone();
		} catch (err) {
			onError(err instanceof ApiError ? err.message : "Oprettelse fejlede.");
			setSubmitting(false);
		}
	};

	return (
		<Dialog
			title="Opret bankkonto"
			onClose={onClose}
			busy={submitting}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}

			<form
				onSubmit={submit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Navn (påkrævet)
					<Input
						disabled={outcome.blocked}
						type="text"
						value={name}
						onChange={(e) => setName(e.target.value)}
						required
						placeholder="fx 'Lunar driftskonto'"
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bank-navn
					<Input
						disabled={outcome.blocked}
						type="text"
						value={bankName}
						onChange={(e) => setBankName(e.target.value)}
						placeholder="fx 'Lunar Bank'"
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Reg.nr.
					<Input
						disabled={outcome.blocked}
						type="text"
						value={registrationNo}
						onChange={(e) => setRegistrationNo(e.target.value)}
						placeholder="4-cifret"
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Konto-nr.
					<Input
						disabled={outcome.blocked}
						type="text"
						value={accountNo}
						onChange={(e) => setAccountNo(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					IBAN
					<Input
						disabled={outcome.blocked}
						type="text"
						value={iban}
						onChange={(e) => setIban(e.target.value)}
						placeholder="DK…"
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					SWIFT/BIC
					<Input
						disabled={outcome.blocked}
						type="text"
						value={bic}
						onChange={(e) => setBic(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Kontoejer
					<Input
						disabled={outcome.blocked}
						type="text"
						value={accountOwner}
						onChange={(e) => setAccountOwner(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bank-kundenr.
					<Input
						disabled={outcome.blocked}
						type="text"
						value={customerNo}
						onChange={(e) => setCustomerNo(e.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Valuta
					<Input
						disabled={outcome.blocked}
						type="text"
						value={currency}
						onChange={(e) => setCurrency(e.target.value.toUpperCase())}
						maxLength={3}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Ledger-konto (valgfri)
					<Input
						disabled={outcome.blocked}
						type="text"
						value={ledgerAccountNo}
						onChange={(e) => setLedgerAccountNo(e.target.value)}
						placeholder="fx 2000 (Bank)"
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
						requiredPermission="company.admin"
						type="submit"
						disabled={outcome.blocked || submitting || !name.trim()}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{submitting ? "Opretter …" : "Opret bankkonto"}
					</Button>
					<Button
						variant="secondary"
						type="button"
						onClick={onClose}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Annullér
					</Button>
				</div>
			</form>
		</Dialog>
	);
}
