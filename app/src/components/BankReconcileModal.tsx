import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Dialog, Select } from "./ui";
// BankReconcileModal — the cockpit's one-click match of an unmatched bank row
// against an open sales invoice (#365).
//
// Without this modal the Bank view marked transactions as `Uafstemt` but
// offered no way to act — the owner had to drop to the CLI/agent to actually
// post the settlement, which is a blocker for a non-technical ApS owner. The
// modal stays on top of the existing `/invoices/settle` write endpoint, so
// every settlement still goes through the same `settleInvoiceWithBankPayment`
// core function the CLI uses; the modal owns nothing more than the picker,
// the busy state and the inline error/lock rendering.

import { useEffect, useRef, useState } from "react";
import { ApiError, api, type InvoiceSettleSummary } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyInvoiceRow } from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** The minimum bank-row context the modal needs to settle it. */
export type BankReconcileTransaction = {
	id: number;
	date: string;
	text: string;
	amount: number;
	currency: string;
};

export type BankReconcileModalProps = {
	/** Company slug the settlement targets. */
	slug: string;
	/** The unmatched bank row the owner is acting on. */
	transaction: BankReconcileTransaction;
	/** Re-runs the Bank view load after a successful settlement. */
	onReconciled: () => void;
	/** Closes the modal without acting. */
	onClose: () => void;
};

type MaybeApiError = { code?: string; message?: string };

// An invoice is "open" — and therefore a valid match for an incoming payment —
// when it still carries an outstanding balance. `paid`/`credited`/`refunded`
// invoices are filtered out so the owner cannot pick something settled.
function isMatchable(inv: CompanyInvoiceRow): boolean {
	return inv.openBalance > 0.005 && inv.status !== "paid";
}

export function BankReconcileModal({
	slug,
	transaction,
	onReconciled,
	onClose: onDismiss,
}: BankReconcileModalProps) {
	const [openInvoices, setOpenInvoices] = useState<CompanyInvoiceRow[] | null>(
		null,
	);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [selectedId, setSelectedId] = useState<number | "">("");
	const [selectionChanged, setSelectionChanged] = useState(false);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [locked, setLocked] = useState<string | null>(null);
	const [done, setDone] = useState<InvoiceSettleSummary | null>(null);
	const closeRef = useRef<HTMLButtonElement>(null);

	const outcome = useMutationOutcome(onReconciled);
	const guard = useDiscardGuard(!done && selectionChanged, onDismiss);
	const { onClose } = guard;

	// Load the company's invoices once; filter to those with an open balance.
	useEffect(() => {
		let cancelled = false;
		api
			.invoices(slug)
			.then((res) => {
				if (cancelled) return;
				const matchable = res.invoices.filter(isMatchable);
				setOpenInvoices(matchable);
				// Pre-select the only candidate if there is exactly one.
				if (matchable.length === 1) setSelectedId(matchable[0]!.documentId);
			})
			.catch((err) => {
				if (cancelled) return;
				const e = err as MaybeApiError;
				setLoadError(e?.message ?? "Fakturaerne kunne ikke hentes.");
			});
		return () => {
			cancelled = true;
		};
	}, [slug]);

	// Move focus into the dialog and let Escape dismiss it — basic modal hygiene.

	async function handleBook() {
		if (outcome.isBlocked()) return;
		if (typeof selectedId !== "number") {
			setError("Vælg en faktura at matche mod.");
			return;
		}
		setBusy(true);
		setError(null);
		setLocked(null);
		try {
			const summary = await outcome.run(() =>
				api.settleInvoice(slug, {
					invoiceDocumentId: selectedId,
					bankTransactionId: transaction.id,
					paymentDate: transaction.date,
				}),
			);
			setDone(summary);
			onReconciled();
		} catch (err) {
			const e = err as MaybeApiError;
			const message = e?.message ?? "Bogføringen kunne ikke gennemføres.";
			if (e?.code === "conflict") setLocked(message);
			else setError(message);
		} finally {
			setBusy(false);
		}
	}

	const currency = transaction.currency || "DKK";
	const noneMatchable = openInvoices !== null && openInvoices.length === 0;

	return (
		<Dialog
			title="Bogfør banktransaktion"
			onClose={onClose}
			busy={busy}
			initialFocusRef={closeRef}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}

			{done ? (
				<>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.modalBody,
						)}
					>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalBodyP,
							)}
						>
							Banktransaktionen blev bogført som betaling af faktura{" "}
							<strong
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{done.invoiceNumber ?? "—"}
							</strong>
							.
						</p>
						{done.openBalance !== null && (
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
									cockpitStyles.modalBodyP,
								)}
							>
								Resterende åben saldo:{" "}
								{formatKroner(done.openBalance, currency)}.
							</p>
						)}
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.modalActions,
						)}
					>
						<Button
							type="button"
							ref={closeRef}
							onClick={onClose}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Luk
						</Button>
					</div>
				</>
			) : (
				<>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.modalBody,
						)}
					>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalBodyP,
							)}
						>
							<strong
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{transaction.date}
							</strong>{" "}
							· {transaction.text} ·{" "}
							{formatKroner(transaction.amount, currency)}
						</p>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
								cockpitStyles.modalBodyP,
							)}
						>
							Vælg den åbne faktura denne banktransaktion betaler. Selve
							postering og bilagsnummer dannes af regnskabskernen — samme vej
							som via kommandolinjen.
						</p>
					</div>

					{locked && <LockBanner message={locked} />}
					{error && <Banner kind="error">{error}</Banner>}
					{loadError && <Banner kind="error">{loadError}</Banner>}

					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.modalField,
						)}
					>
						Match mod faktura
						{openInvoices === null ? (
							<Select
								disabled
								xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
							>
								<option
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Henter fakturaer…
								</option>
							</Select>
						) : noneMatchable ? (
							<Select
								disabled
								xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
							>
								<option
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Ingen åbne fakturaer at matche mod
								</option>
							</Select>
						) : (
							<Select
								value={selectedId === "" ? "" : String(selectedId)}
								onChange={(e) => {
									setSelectionChanged(true);
									const v = e.target.value;
									setSelectedId(v === "" ? "" : Number(v));
								}}
								disabled={outcome.blocked || busy}
								xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
							>
								<option
									value=""
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									— vælg faktura —
								</option>
								{openInvoices.map((inv) => (
									<option
										key={inv.documentId}
										value={String(inv.documentId)}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{inv.invoiceNo}
										{inv.customerName ? ` · ${inv.customerName}` : ""} ·{" "}
										{formatKroner(inv.openBalance, inv.currency)} åben
									</option>
								))}
							</Select>
						)}
					</label>

					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.modalActions,
						)}
					>
						<Button
							variant="secondary"
							type="button"
							onClick={onClose}
							disabled={busy}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Annullér
						</Button>
						<Button
							requiredPermission="company.ledger.post"
							type="button"
							onClick={handleBook}
							disabled={
								outcome.blocked ||
								busy || noneMatchable || openInvoices === null ||
								typeof selectedId !== "number"
							}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							{busy ? "Bogfører…" : "Bogfør"}
						</Button>
					</div>
				</>
			)}
		</Dialog>
	);
}

// `ApiError` is re-imported above purely so the build tracks the dependency;
// the runtime branch reads `code`/`message` off the thrown value rather than
// instance-checking, mirroring `BankImportModal`'s shape.
void ApiError;
