import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import type { BankAccount } from "../lib/types";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Dialog, Input, Select } from "./ui";

type Props = {
	slug: string;
	accounts: BankAccount[];
	onApplied: () => void;
	onClose: () => void;
};
/** Controlled one-time bank binding: this screen never offers a remap. */
export function LegacyBankBindingModal({
	slug,
	accounts,
	onApplied,
	onClose: onDismiss,
}: Props) {
	const [bankAccountId, setBankAccountId] = useState(
		accounts.find((a) => a.ledgerAccountNo === null)?.id ?? 0,
	);
	const [ledgerAccountNo, setLedgerAccountNo] = useState("");
	const [cutoff, setCutoff] = useState("");
	const [planHash, setPlanHash] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const input = { bankAccountId, ledgerAccountNo, cutoff };
	const review = async () => {
		setBusy(true);
		setError(null);
		try {
			setPlanHash((await api.planLegacyBinding(slug, input)).planHash);
		} catch (e) {
			setError(e instanceof Error ? e.message : "Planen kunne ikke oprettes");
		} finally {
			setBusy(false);
		}
	};
	const outcome = useMutationOutcome(onApplied);
	const guard = useDiscardGuard(
		Boolean(ledgerAccountNo || cutoff || planHash),
		onDismiss,
	);
	const { onClose } = guard;
	const operationKey = useRef<string>();
	const apply = async () => {
		if (outcome.isBlocked()) return;
		if (!operationKey.current) operationKey.current = crypto.randomUUID();
		setBusy(true);
		setError(null);
		try {
			await outcome.run(() =>
				api.applyLegacyBinding(slug, {
					...input,
					planHash,
					idempotencyKey: operationKey.current!,
				}),
			);
			onApplied();
			guard.dismiss();
		} catch (e) {
			setError(
				e instanceof Error ? e.message : "Binding kunne ikke gennemføres",
			);
		} finally {
			setBusy(false);
		}
	};
	const ready = bankAccountId > 0 && ledgerAccountNo.trim() && cutoff;
	return (
		<Dialog
			title="Bind tidligere uforbundet bankkonto"
			onClose={onClose}
			busy={busy}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.pageHead,
				)}
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
						Kun NULL → bekræftet bankkonto. En eksisterende binding kan aldrig
						ændres her.
					</p>
				</div>
				<Button
					type="button"
					onClick={onClose}
					variant={"secondary"}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Luk
				</Button>
			</div>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.formGrid,
				)}
			>
				<label
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}
				>
					Bankkonto
					<Select
						value={bankAccountId}
						onChange={(e) => {
							setBankAccountId(Number(e.target.value));
							setPlanHash("");
						}}
						xstyle={[cockpitStyles.selectComposition]}
					>
						{accounts
							.filter((a) => a.ledgerAccountNo === null)
							.map((a) => (
								<option
									key={a.id}
									value={a.id}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{a.name} · #{a.id}
								</option>
							))}
					</Select>
				</label>
				<label
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}
				>
					Finanskonto
					<Input
						value={ledgerAccountNo}
						onChange={(e) => {
							setLedgerAccountNo(e.target.value);
							setPlanHash("");
						}}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}
				>
					Cutoff
					<Input
						type="date"
						value={cutoff}
						onChange={(e) => {
							setCutoff(e.target.value);
							setPlanHash("");
						}}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
			</div>
			{error && (
				<p
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</p>
			)}
			{planHash && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Reviewet plan {planHash.slice(0, 12)}…
				</p>
			)}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.rowActions,
				)}
			>
				<Button
					type="button"
					disabled={!ready || busy}
					onClick={review}
					variant={"secondary"}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					{busy ? "Arbejder…" : "Opret plan"}
				</Button>
				<Button
					type="button"
					requiredPermission="company.ledger.post"
					disabled={outcome.blocked || !planHash || busy}
					onClick={apply}
					variant={"danger"}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Bekræft binding
				</Button>
			</div>
		</Dialog>
	);
}
