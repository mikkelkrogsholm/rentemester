import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";
// DocumentBookExpenseModal — the cockpit's one-click bogføring of an
// "Ikke bogført" bilag against an unmatched outgoing bank transaction (#407).
//
// Without this modal the Bilag view marked a row as `Ikke bogført` but
// offered no way to act — the owner had to drop to the CLI to actually post
// the expense, which is a blocker for a non-technical ApS owner. The modal
// stays on top of the existing `/documents/book-expense` write endpoint, so
// every booking still goes through the same `bookExpenseFromBank` core
// function the CLI's `expense book` command uses; the modal owns nothing
// more than the picker, the busy state and the inline error/lock rendering.
//
// The "konfirmation"-step is the modal itself: the owner sees the bilag's
// fields, the chosen expense account, the chosen bank transaction and the
// computed net/VAT/gross from the bilag before the "Bogfør"-button is
// pressed — the same determinism contract the CLI honours.

import { useEffect, useRef, useState } from "react";
import {
	ApiError,
	api,
	type DocumentBookExpenseSummary,
	type DocumentBookingOptions,
	type DocumentVatPreflight,
	type ExpenseVatTreatment,
} from "../lib/api";
import { formatKroner } from "../lib/format";
import type { DocumentRow } from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";
import { Button, Dialog, Select } from "./ui";

export type DocumentBookExpenseModalProps = {
	slug: string;
	/** The bilag id the action targets. */
	documentId: number;
	/** Re-runs the Bilag view load after a successful booking. */
	onBooked: () => void;
	/** Closes the modal without acting. */
	onClose: () => void;
};

type MaybeApiError = { code?: string; message?: string };

const VAT_TREATMENT_LABELS: Record<ExpenseVatTreatment, string> = {
	standard: "Standard (25% købsmoms)",
	reverse_charge: "Omvendt betalingspligt (udenlandsk ydelse)",
	representation: "Repræsentation (delvis fradragsret)",
	exempt: "Momsfri",
	non_deductible: "Ikke fradragsberettiget (momsen absorberes i udgiften)",
};

export function DocumentBookExpenseForm({
	slug,
	documentId,
	onBooked,
	onClose,
	presentation = "dialog",
}: DocumentBookExpenseModalProps & { presentation?: "dialog" | "page" }) {
	const optionsState = useAsync<DocumentBookingOptions>(
		(signal) => api.documentBookingOptions(slug, documentId, { signal }),
		[slug, documentId],
	);
	const options = optionsState.data;
	const loadError = optionsState.error;
	const [expenseAccountNo, setExpenseAccountNo] = useState<string>("");
	const [bankTransactionId, setBankTransactionId] = useState<number | "">("");
	const [vatTreatment, setVatTreatment] = useState<ExpenseVatTreatment | "">(
		"",
	);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [locked, setLocked] = useState<string | null>(null);
	const [done, setDone] = useState<DocumentBookExpenseSummary | null>(null);
	const [preflight, setPreflight] = useState<DocumentVatPreflight | null>(null);
	const [preflightBusy, setPreflightBusy] = useState(false);
	const closeRef = useRef<HTMLButtonElement>(null);
	const [dirty, setDirty] = useState(false);
	const [readBack, setReadBack] = useState<DocumentRow[] | null>(null);
	const outcome = useMutationOutcome(async () => {
		const documents = await api.documents(slug);
		setReadBack(
			documents.documents.filter((document) => document.id === documentId),
		);
	}, `book-document:${slug}:${documentId}`);
	const uncertain = outcome.blocked;
	const submitRef = useRef(false);
	const guard = useDiscardGuard(dirty && !done, onClose);

	// Server options remain the source for all account/bank choices and hints.
	useEffect(() => {
		if (!options) return;
		const res = options;
		if (
			res.document.documentType === "internal_voucher" &&
			res.document.sourceBankTransactionId !== null
		) {
			setBankTransactionId(res.document.sourceBankTransactionId);
			setVatTreatment("exempt");
			return;
		}
		const gross = res.document.amountIncVat;
		if (gross !== null) {
			const exact = res.unmatchedOutgoingBank.filter((transaction) => {
				const documentCurrency = res.document.currency.toUpperCase();
				const bankCurrency = transaction.currency.toUpperCase();
				if (bankCurrency === documentCurrency)
					return (
						Math.abs(Math.abs(transaction.amount) - Math.abs(gross)) < 0.005
					);
				if (
					bankCurrency === "DKK" &&
					documentCurrency !== "DKK" &&
					transaction.fxRateToDkk &&
					transaction.fxRateToDkk > 0
				)
					return (
						Math.abs(
							Math.abs(transaction.amount) -
								Math.abs(gross * transaction.fxRateToDkk),
						) < 0.005
					);
				return false;
			});
			if (exact.length === 1)
				setBankTransactionId((previous) =>
					previous === "" ? exact[0]!.id : previous,
				);
		}
	}, [options]);

	useEffect(() => {
		let cancelled = false;
		api
			.documentVatPreflight(slug, documentId)
			.then((result) => {
				if (!cancelled) setPreflight(result);
			})
			.catch(() => {
				/* Booking remains available; its core boundary still fails closed. */
			});
		return () => {
			cancelled = true;
		};
	}, [slug, documentId]);

	async function applyPreflight() {
		if (outcome.isBlocked() || preflightBusy) return;
		setPreflightBusy(true);
		setError(null);
		try {
			setPreflight(
				await outcome.run(() =>
					api.applyDocumentVatPreflight(slug, documentId),
				),
			);
		} catch (err) {
			setError(
				(err as MaybeApiError)?.message ??
					"Momsvalideringen kunne ikke gennemføres.",
			);
		} finally {
			setPreflightBusy(false);
		}
	}

	async function handleBook() {
		if (submitRef.current || done || outcome.isBlocked()) return;
		if (!expenseAccountNo) {
			setError("Vælg en udgiftskonto.");
			return;
		}
		if (typeof bankTransactionId !== "number") {
			setError("Vælg en banktransaktion at parre bilaget med.");
			return;
		}
		submitRef.current = true;
		setBusy(true);
		setError(null);
		setLocked(null);
		try {
			const summary = await outcome.run(() =>
				api.bookDocumentExpense(slug, {
					documentId,
					bankTransactionId,
					expenseAccountNo,
					...(vatTreatment ? { vatTreatment } : {}),
				}),
			);
			setDone(summary);
			setDirty(false);
			onBooked();
		} catch (err) {
			const e = err as MaybeApiError;
			const outcomeUncertain = e?.code === "network" || e?.code === "internal";
			const message = outcomeUncertain
				? "Serverens resultat kunne ikke bekræftes. Kontrollér bilaget og posteringerne, før du bogfører igen."
				: (e?.message ?? "Bogføringen kunne ikke gennemføres.");
			if (e?.code === "conflict") setLocked(message);
			else setError(message);
		} finally {
			submitRef.current = false;
			setBusy(false);
		}
	}

	const doc = options?.document;
	const currency = doc?.currency || "DKK";
	const noneToMatch =
		options !== null && options.unmatchedOutgoingBank.length === 0;
	const noAccounts = options !== null && options.expenseAccounts.length === 0;

	return (
		<Dialog
			title="Bogfør bilag"
			onClose={guard.onClose}
			busy={busy}
			mode={presentation}
			initialFocusRef={closeRef}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			<div
				onChange={() => setDirty(true)}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.workflowForm,
				)}
				data-ui="workflow-form"
			>
				{done ? (
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalBody,
								cockpitStyles.workflowForm2,
							)}
						>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.modalBodyP,
								)}
							>
								Bilaget blev bogført som journalpost{" "}
								<strong
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{done.entryId ?? "—"}
								</strong>
								.
							</p>
							{done.grossAmount !== null && (
								<p
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.muted,
										cockpitStyles.modalBodyP,
									)}
								>
									Bruttobeløb: {formatKroner(done.grossAmount, currency)} ·
									{currency !== "DKK" && done.grossAmountDkk !== null && (
										<> {formatKroner(done.grossAmountDkk, "DKK")} ·</>
									)}
									Nettobeløb:{" "}
									{done.netAmountDkk !== null || done.netAmount !== null
										? formatKroner(done.netAmountDkk ?? done.netAmount!, "DKK")
										: "—"}{" "}
									· Købsmoms:{" "}
									{done.vatAmountDkk !== null || done.vatAmount !== null
										? formatKroner(done.vatAmountDkk ?? done.vatAmount!, "DKK")
										: "—"}
									.
								</p>
							)}
						</div>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.workflowForm2,
								cockpitStyles.modalActions,
							)}
						>
							<Button
								type="button"
								ref={closeRef}
								onClick={guard.dismiss}
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
								cockpitStyles.workflowForm2,
							)}
						>
							{options === null && !loadError && (
								<p
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.muted,
										cockpitStyles.modalBodyP,
									)}
								>
									Henter bogføringsdata…
								</p>
							)}
							{doc && (
								<>
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
											{doc.documentType === "internal_voucher"
												? "Internt bilag"
												: (doc.supplierName ?? "Ukendt leverandør")}
										</strong>
										{doc.invoiceNo ? ` · faktura ${doc.invoiceNo}` : ""} ·{" "}
										{doc.invoiceDate ?? "—"} ·{" "}
										{doc.amountIncVat !== null
											? formatKroner(doc.amountIncVat, currency)
											: "—"}{" "}
										inkl. moms
									</p>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
											cockpitStyles.modalBodyP,
										)}
									>
										Vælg den udgiftskonto bilaget hører til.{" "}
										{doc.documentType === "internal_voucher"
											? "Banktransaktionen og den momsfrie behandling er låst til bilagets evidens. "
											: "Vælg også den banktransaktion det betaler. "}
										Selve posteringen og bilagets moms-beregning dannes af
										regnskabskernen — samme vej som via kommandolinjen.
									</p>
									{(doc.supplierCountryCode ||
										doc.supplierIdentifierKind ||
										doc.supplierIdentityStatus) && (
										<p
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
												cockpitStyles.modalBodyP,
											)}
										>
											Leverandøridentitet: {doc.supplierCountryCode ?? "—"} ·{" "}
											{doc.supplierIdentifierKind ?? "—"} ·{" "}
											{doc.supplierIdentityStatus ?? "—"}
											{doc.supplierVatOrCvr ? ` · ${doc.supplierVatOrCvr}` : ""}
										</p>
									)}
									{doc.purchaseVatLines && doc.purchaseVatLines.length > 0 && (
										<div
											role="group"
											aria-label="Momsfordeling"
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											Momsfordeling:{" "}
											{doc.purchaseVatLines
												.map(
													(line) =>
														`${line.classification}: ${formatKroner(line.netAmount, currency)} + ${formatKroner(line.vatAmount ?? 0, currency)}`,
												)
												.join(" · ")}
										</div>
									)}
									{preflight && (
										<div
											role="status"
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											Momsvalidering:{" "}
											{
												{
													DK: "Danmark",
													EU: "EU",
													NON_EU: "Uden for EU",
													CONFLICT: "Modstridende landeoplysninger",
												}[preflight.derivedRegion]
											}
											{preflight.requiredValidation
												? ` · kræver ${preflight.requiredValidation}`
												: " · ingen ekstern validering kræves"}
											{preflight.cache.freshUntil
												? ` · evidens gyldig til ${preflight.cache.freshUntil}`
												: ""}
											.
											{preflight.errors[0]
												? ` ${preflight.errors[0]}`
												: preflight.ok
													? " Momsoplysningerne er valideret."
													: " Momsoplysningerne skal afklares før bogføring."}
											{preflight.applyWouldCallProvider && (
												<Button
													type="button"
													disabled={busy || preflightBusy}
													requiredPermission="company.external-lookup"
													onClick={() => void applyPreflight()}
													xstyle={[cockpitStyles.buttonComposition]}
												>
													{preflightBusy ? "Validerer…" : "Hent momsvalidering"}
												</Button>
											)}
										</div>
									)}
								</>
							)}
						</div>

						{locked && <LockBanner message={locked} />}
						{error && <Banner kind="error">{error}</Banner>}
						{outcome.feedback}
						{readBack && uncertain && (
							<section
								role="status"
								aria-label="Bilagsstatus fra serveren"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.card,
									cockpitStyles.workflowForm2,
								)}
							>
								<h2
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.h2,
									)}
								>
									Bilagsstatus fra serveren
								</h2>
								{readBack.some(
									(document) => document.journalEntryId !== null,
								) ? (
									<>
										<p
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Serveren viser følgende tilknyttede posteringer.
											Kontrollér, at de svarer til din handling.
										</p>
										<ul
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{readBack
												.filter((document) => document.journalEntryId !== null)
												.map((document, index) => (
													<li
														key={`${document.journalEntryId}:${index}`}
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}
													>
														{document.journalEntryNo ??
															`Journalpost ${document.journalEntryId}`}{" "}
														·{" "}
														{document.journalEntryText ??
															"Ingen posteringstekst"}
													</li>
												))}
										</ul>
									</>
								) : (
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Serveren viser endnu ingen tilknyttede posteringer. Det
										afklarer ikke i sig selv den afbrudte handling.
									</p>
								)}
								<p
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Bogføring er fortsat blokeret. Gennemgå bilagsdetaljerne og
									revisionssporet ved tvivl.
								</p>
							</section>
						)}
						{loadError && (
							<Banner kind="error">
								{loadError}{" "}
								<Button
									variant="secondary"
									onClick={optionsState.reload}
									xstyle={[cockpitStyles.buttonComposition]}
								>
									Prøv igen
								</Button>
							</Banner>
						)}

						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalField,
								cockpitStyles.workflowForm2,
							)}
						>
							Udgiftskonto
							{options === null ? (
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
										Henter konti…
									</option>
								</Select>
							) : noAccounts ? (
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
										Ingen udgiftskonti — kør først kontoplanen.
									</option>
								</Select>
							) : (
								<Select
									value={expenseAccountNo}
									onChange={(e) => setExpenseAccountNo(e.target.value)}
									disabled={busy || uncertain}
									xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
								>
									<option
										value=""
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										— vælg konto —
									</option>
									{options.expenseAccounts.map((a) => (
										<option
											key={a.accountNo}
											value={a.accountNo}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{a.accountNo} · {a.name}
										</option>
									))}
								</Select>
							)}
						</label>

						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalField,
								cockpitStyles.workflowForm2,
							)}
						>
							Banktransaktion (uafstemt, udgående)
							{options === null ? (
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
										Henter banktransaktioner…
									</option>
								</Select>
							) : noneToMatch ? (
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
										Ingen uafstemte udgående banktransaktioner — importér først
										bank-CSV.
									</option>
								</Select>
							) : (
								<Select
									value={
										bankTransactionId === "" ? "" : String(bankTransactionId)
									}
									onChange={(e) => {
										const v = e.target.value;
										setBankTransactionId(v === "" ? "" : Number(v));
									}}
									disabled={
										busy ||
										uncertain ||
										doc?.documentType === "internal_voucher"
									}
									xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
								>
									<option
										value=""
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										— vælg banktransaktion —
									</option>
									{options.unmatchedOutgoingBank.map((t) => (
										<option
											key={t.id}
											value={String(t.id)}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{t.date} · {t.text} · {formatKroner(t.amount, t.currency)}
										</option>
									))}
								</Select>
							)}
						</label>

						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalField,
								cockpitStyles.workflowForm2,
							)}
						>
							Moms-behandling (valgfri — udledes ellers af kontoen)
							<Select
								value={vatTreatment}
								onChange={(e) =>
									setVatTreatment(
										e.target.value === ""
											? ""
											: (e.target.value as ExpenseVatTreatment),
									)
								}
								disabled={
									busy ||
									uncertain ||
									preflight?.ok === false ||
									preflightBusy ||
									options === null ||
									doc?.documentType === "internal_voucher"
								}
								xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
							>
								<option
									value=""
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									— udled fra konto —
								</option>
								{(
									Object.keys(VAT_TREATMENT_LABELS) as ExpenseVatTreatment[]
								).map((k) => (
									<option
										key={k}
										value={k}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{VAT_TREATMENT_LABELS[k]}
									</option>
								))}
							</Select>
						</label>

						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.workflowForm2,
								cockpitStyles.modalActions,
							)}
						>
							<Button
								type="button"
								onClick={guard.onClose}
								disabled={busy || uncertain}
								ref={closeRef}
								variant={"secondary"}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Annullér
							</Button>
							<Button
								type="button"
								requiredPermission="company.ledger.post"
								onClick={handleBook}
								disabled={
									busy ||
									uncertain ||
									preflight?.ok === false ||
									preflightBusy ||
									options === null ||
									noneToMatch ||
									noAccounts ||
									!expenseAccountNo ||
									typeof bankTransactionId !== "number"
								}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								{busy ? "Bogfører…" : "Bogfør"}
							</Button>
						</div>
					</>
				)}
			</div>
			{guard.confirmation}
		</Dialog>
	);
}

// `ApiError` is re-imported above purely so the build tracks the dependency;
// the runtime branch reads `code`/`message` off the thrown value rather than
// instance-checking, mirroring `BankReconcileModal`'s shape.
void ApiError;

/** Compatibility wrapper for callers that still need a short dialog. */
export function DocumentBookExpenseModal(props: DocumentBookExpenseModalProps) {
	return <DocumentBookExpenseForm {...props} presentation="dialog" />;
}
