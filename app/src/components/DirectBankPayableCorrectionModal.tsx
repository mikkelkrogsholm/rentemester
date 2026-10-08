import * as stylex from "@stylexjs/stylex";
import { useMemo, useRef, useState } from "react";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import type {
	CompanyPayables,
	DirectBankPayableCorrectionInput,
	DirectBankPayableCorrectionPlan,
} from "../lib/types";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Dialog, Input, Select } from "./ui";

type Props = {
	slug: string;
	payables: CompanyPayables;
	onApplied: () => void;
	onClose: () => void;
};

/** Two-step reviewed correction for a purchase that was booked directly to bank. */
export function DirectBankPayableCorrectionModal({
	slug,
	payables,
	onApplied,
	onClose: onDismiss,
}: Props) {
	const document = payables.unregisteredDocuments[0];
	const account = payables.expenseAccounts[0];
	const [documentId, setDocumentId] = useState(document?.id ?? 0);
	const selected = useMemo(
		() => payables.unregisteredDocuments.find((row) => row.id === documentId),
		[documentId, payables.unregisteredDocuments],
	);
	const [bankTransactionId, setBankTransactionId] = useState("");
	const [billDate, setBillDate] = useState(document?.invoiceDate ?? "");
	const [dueDate, setDueDate] = useState(document?.invoiceDate ?? "");
	const [expenseAccountNo, setExpenseAccountNo] = useState(
		account?.accountNo ?? "",
	);
	const [reason, setReason] = useState(
		"Ret direkte bankkøb til kreditorforløb",
	);
	const [plan, setPlan] = useState<DirectBankPayableCorrectionPlan | null>(
		null,
	);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const dirty =
		Boolean(bankTransactionId || plan) ||
		documentId !== (document?.id ?? 0) ||
		dueDate !== (document?.invoiceDate ?? "") ||
		expenseAccountNo !== (account?.accountNo ?? "") ||
		reason !== "Ret direkte bankkøb til kreditorforløb";
	const outcome = useMutationOutcome(onApplied);
	const guard = useDiscardGuard(dirty, onDismiss);
	const { onClose } = guard;
	const correctionKey = useRef<string>();
	const input = (): DirectBankPayableCorrectionInput => ({
		documentId,
		bankTransactionId: Number(bankTransactionId),
		billDate,
		dueDate,
		expenseAccountNo,
		vatTreatment: "standard",
	});
	const review = async () => {
		setBusy(true);
		setError(null);
		try {
			setPlan(await api.planDirectBankPayableCorrection(slug, input()));
		} catch (e) {
			setError(e instanceof Error ? e.message : "Planen kunne ikke oprettes");
		} finally {
			setBusy(false);
		}
	};
	const apply = async () => {
		if (outcome.isBlocked()) return;
		if (!plan) return;
		if (!correctionKey.current) correctionKey.current = crypto.randomUUID();
		setBusy(true);
		setError(null);
		try {
			await outcome.run(() =>
				api.applyDirectBankPayableCorrection(slug, {
					...input(),
					planHash: plan.planHash,
					reason,
					idempotencyKey: correctionKey.current!,
				}),
			);
			onApplied();
			guard.dismiss();
		} catch (e) {
			setError(
				e instanceof Error ? e.message : "Korrektionen kunne ikke gennemføres",
			);
		} finally {
			setBusy(false);
		}
	};
	return (
		<Dialog
			title="Ret direkte bankkøb"
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
						Bevar fakturadatoen, opret kreditorposten og flyt bankafregningen
						til bankdatoen.
					</p>
				</div>
				<Button
					variant="secondary"
					type="button"
					onClick={onClose}
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
					Bilag
					<Select
						disabled={outcome.blocked}
						value={documentId}
						onChange={(event) => {
							const id = Number(event.target.value);
							setDocumentId(id);
							setBillDate(
								payables.unregisteredDocuments.find((row) => row.id === id)
									?.invoiceDate ?? "",
							);
							setPlan(null);
						}}
						xstyle={[cockpitStyles.selectComposition]}
					>
						{payables.unregisteredDocuments.map((row) => (
							<option
								key={row.id}
								value={row.id}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{row.invoiceNo ?? row.documentNo ?? `Bilag ${row.id}`}
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
					Banktransaktions-id
					<Input
						disabled={outcome.blocked}
						inputMode="numeric"
						value={bankTransactionId}
						onChange={(event) => {
							setBankTransactionId(event.target.value);
							setPlan(null);
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
					Fakturadato
					<Input
						disabled={outcome.blocked}
						type="date"
						value={billDate}
						readOnly
						aria-describedby="invoice-date-note"
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
					Forfaldsdato
					<Input
						disabled={outcome.blocked}
						type="date"
						value={dueDate}
						onChange={(event) => {
							setDueDate(event.target.value);
							setPlan(null);
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
					Udgiftskonto
					<Select
						disabled={outcome.blocked}
						value={expenseAccountNo}
						onChange={(event) => {
							setExpenseAccountNo(event.target.value);
							setPlan(null);
						}}
						xstyle={[cockpitStyles.selectComposition]}
					>
						{payables.expenseAccounts.map((row) => (
							<option
								key={row.accountNo}
								value={row.accountNo}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{row.accountNo} · {row.name}
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
					Begrundelse
					<Input
						disabled={outcome.blocked}
						value={reason}
						onChange={(event) => setReason(event.target.value)}
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
			</div>
			<p
				id="invoice-date-note"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Fakturadatoen kommer uændret fra bilaget {selected?.invoiceNo ?? ""}.
			</p>
			{error && (
				<p
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</p>
			)}
			{plan && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kontrollér planen
					</strong>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Bankdato {plan.bankDate} · {plan.bankAmount.toFixed(2)} DKK · plan{" "}
						{plan.planHash.slice(0, 12)}…
					</p>
				</div>
			)}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.rowActions,
				)}
			>
				<Button
					variant="secondary"
					type="button"
					disabled={
						busy ||
						!documentId ||
						!Number(bankTransactionId) ||
						!billDate ||
						!dueDate ||
						!expenseAccountNo
					}
					onClick={review}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					{busy ? "Arbejder…" : "Opret plan"}
				</Button>
				<Button
					requiredPermission="company.ledger.post"
					variant="danger"
					type="button"
					disabled={outcome.blocked || busy || !plan || !reason.trim()}
					onClick={apply}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Bekræft korrektion
				</Button>
			</div>
		</Dialog>
	);
}
