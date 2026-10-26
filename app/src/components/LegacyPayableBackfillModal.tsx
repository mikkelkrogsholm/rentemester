import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import type {
	LegacyPayableBackfillInput,
	LegacyPayableBackfillPlan,
} from "../lib/types";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Dialog, Input } from "./ui";

type Props = { slug: string; onApplied: () => void; onClose: () => void };
/** A deliberately explicit cockpit surface: IDs are entered, never discovered by amount. */
export function LegacyPayableBackfillModal({
	slug,
	onApplied,
	onClose: onDismiss,
}: Props) {
	const [input, setInput] = useState<LegacyPayableBackfillInput>({
		purchaseJournalEntryId: 0,
		paymentJournalEntryId: 0,
		documentId: 0,
		bankTransactionId: 0,
	});
	const [plan, setPlan] = useState<LegacyPayableBackfillPlan | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const update = (key: keyof LegacyPayableBackfillInput, value: string) => {
		setInput({ ...input, [key]: Number(value) });
		setPlan(null);
	};
	const review = async () => {
		setBusy(true);
		setError(null);
		try {
			setPlan(await api.planLegacyPayableBackfill(slug, input));
		} catch (e) {
			setError(e instanceof Error ? e.message : "Planen kunne ikke oprettes");
		} finally {
			setBusy(false);
		}
	};
	const outcome = useMutationOutcome(onApplied);
	const guard = useDiscardGuard(
		Object.values(input).some((value) => value > 0),
		onDismiss,
	);
	const { onClose } = guard;
	const operationKey = useRef<string>();
	const apply = async () => {
		if (outcome.isBlocked()) return;
		if (!operationKey.current) operationKey.current = crypto.randomUUID();
		if (!plan) return;
		setBusy(true);
		setError(null);
		try {
			await outcome.run(() =>
				api.applyLegacyPayableBackfill(slug, {
					...input,
					planHash: plan.planHash,
					idempotencyKey: operationKey.current!,
				}),
			);
			onApplied();
			guard.dismiss();
		} catch (e) {
			setError(
				e instanceof Error ? e.message : "Backfill kunne ikke gennemføres",
			);
		} finally {
			setBusy(false);
		}
	};
	const ready = Object.values(input).every((n) => Number.isInteger(n) && n > 0);
	return (
		<Dialog
			title="Adoptér eksisterende kreditorbetaling"
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
						Kun for verificerede migrerede data. Systemet søger aldrig på beløb.
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
				{(
					[
						["purchaseJournalEntryId", "Købsjournal-id"],
						["paymentJournalEntryId", "Betalingsjournal-id"],
						["documentId", "Bilags-id"],
						["bankTransactionId", "Banktransaktions-id"],
					] as const
				).map(([key, label]) => (
					<label
						key={key}
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.formGridLabel,
						)}
					>
						{label}
						<Input
							inputMode="numeric"
							value={input[key] || ""}
							onChange={(e) => update(key, e.target.value)}
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</label>
				))}
			</div>
			{error && (
				<p
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{error}
				</p>
			)}
			{plan && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Reviewet plan {plan.planHash.slice(0, 12)}… binder præcis de
					indtastede identiteter.
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
					disabled={outcome.blocked || !plan || busy}
					onClick={apply}
					variant={"danger"}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Bekræft backfill
				</Button>
			</div>
		</Dialog>
	);
}
