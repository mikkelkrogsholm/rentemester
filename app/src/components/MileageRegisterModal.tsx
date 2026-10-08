import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Dialog, Input, Textarea } from "./ui";
// MileageRegisterModal — the human "Registrér kørsel" action for the Cockpit
// (#335).
//
// A person opens this from the Kørsel view, fills in the trip's date, purpose,
// from/to, kilometres, vehicle, driver, per-km rate, and the rate-basis note
// (which official rate table the rate came from). The browser POSTs only
// those essentials; the server runs the SAME `createMileageEntry` core path
// the CLI's `mileage add` command uses. The mileage register is append-only
// audit data — the schema's triggers refuse any update or delete — so the
// body carries `confirm: true`.

import { useRef, useState } from "react";
import { api } from "../lib/api";
import { formatKroner, parseDanishAmount, todayIso } from "../lib/format";
import type { MileageEntrySummary } from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type MileageRegisterModalProps = {
	slug: string;
	/** Called after a successful create so the calling view can refresh. */
	onRegistered: () => void;
	/** Closes the modal without acting. */
	onClose: () => void;
};

export function MileageRegisterModal({
	slug,
	onRegistered,
	onClose: onDismiss,
}: MileageRegisterModalProps) {
	const [tripDate, setTripDate] = useState(todayIso());
	const [purpose, setPurpose] = useState("");
	const [fromLocation, setFromLocation] = useState("");
	const [toLocation, setToLocation] = useState("");
	const [kilometers, setKilometers] = useState("");
	const [vehicle, setVehicle] = useState("Privat bil");
	const [driver, setDriver] = useState("");
	const [ratePerKm, setRatePerKm] = useState("");
	// The mileage core deliberately does NOT own a tax rate: `rateBasis` is the
	// free-text, source-backed note the human confirms (e.g. "SKAT
	// befordringsfradrag 2026 (høj sats, op til 20.000 km)"). The link to
	// skat.dk is rendered next to the field as a guidance touchpoint.
	const [rateBasis, setRateBasis] = useState("");
	const [rateSource, setRateSource] = useState("");
	const [notes, setNotes] = useState("");

	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [locked, setLocked] = useState<string | null>(null);
	const [done, setDone] = useState<MileageEntrySummary | null>(null);
	const closeRef = useRef<HTMLButtonElement>(null);
	const firstFieldRef = useRef<HTMLInputElement>(null);

	const outcome = useMutationOutcome(onRegistered);
	const guard = useDiscardGuard(
		!done &&
			(Boolean(
				purpose ||
					fromLocation ||
					toLocation ||
					kilometers ||
					driver ||
					ratePerKm ||
					rateBasis ||
					rateSource ||
					notes,
			) ||
				tripDate !== todayIso() ||
				vehicle !== "Privat bil"),
		onDismiss,
	);
	const { onClose } = guard;

	function reset() {
		// Keep the vehicle + rate-basis fields so repeat trips are quick to log.
		setPurpose("");
		setFromLocation("");
		setToLocation("");
		setKilometers("");
		setNotes("");
		setError(null);
		setLocked(null);
		setDone(null);
	}

	async function handleSubmit(event: React.FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		setError(null);
		setLocked(null);

		const km = parseDanishAmount(kilometers);
		const rate = parseDanishAmount(ratePerKm);
		if (!tripDate) return setError("Dato er påkrævet.");
		if (!purpose.trim()) return setError("Formål er påkrævet.");
		if (!fromLocation.trim()) return setError("Fra-adresse er påkrævet.");
		if (!toLocation.trim()) return setError("Til-adresse er påkrævet.");
		if (km === null || km <= 0)
			return setError("Antal km skal være et positivt tal.");
		if (!vehicle.trim()) return setError("Køretøj er påkrævet.");
		if (!driver.trim()) return setError("Chauffør er påkrævet.");
		if (rate === null || rate <= 0)
			return setError("Takst skal være et positivt tal i kr/km.");
		if (!rateBasis.trim())
			return setError(
				"Takst-grundlag er påkrævet — angiv hvilken officiel sats taksten kommer fra (fx 'SKAT 2026, høj sats').",
			);

		setBusy(true);
		try {
			const result = await outcome.run(() =>
				api.createMileageEntry(slug, {
					tripDate,
					purpose: purpose.trim(),
					fromLocation: fromLocation.trim(),
					toLocation: toLocation.trim(),
					kilometers: km,
					vehicle: vehicle.trim(),
					driver: driver.trim(),
					ratePerKm: rate,
					rateBasis: rateBasis.trim(),
					...(rateSource.trim() ? { rateSource: rateSource.trim() } : {}),
					...(notes.trim() ? { notes: notes.trim() } : {}),
				}),
			);
			setDone(result);
			onRegistered();
		} catch (err) {
			const e = err as MaybeApiError;
			// The backup-lock conflict carries a curated Danish message — render it
			// in the dedicated banner so the human gets a calm explanation, not a
			// red error.
			if (
				e?.code === "conflict" &&
				/[Bb]ogføring er låst/.test(e.message ?? "")
			) {
				setLocked(e.message!);
			} else {
				setError(e?.message ?? "Kørslen kunne ikke registreres.");
			}
		} finally {
			setBusy(false);
		}
	}

	const kmPreview = parseDanishAmount(kilometers);
	const ratePreview = parseDanishAmount(ratePerKm);
	const amountBasis =
		kmPreview !== null && ratePreview !== null ? kmPreview * ratePreview : null;

	return (
		<Dialog
			title="Registrér kørsel"
			onClose={onClose}
			busy={busy}
			initialFocusRef={firstFieldRef}
			xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
		>
			{outcome.feedback}
			{guard.confirmation}

			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalHead,
				)}
			>
				<Button
					variant="secondary"
					type="button"
					ref={closeRef}
					onClick={onClose}
					disabled={busy}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Luk
				</Button>
			</div>

			{locked && <LockBanner message={locked} />}
			{error && <Banner kind="error">{error}</Banner>}

			{done ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.modalBody,
					)}
				>
					<Banner kind="success">
						Kørsel registreret som {done.entryNo} —{" "}
						{done.amountBasis !== null
							? formatKroner(done.amountBasis, "DKK")
							: "—"}{" "}
						i godtgørelsesgrundlag.
					</Banner>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rowActions,
							viewStyles.site0,
						)}
					>
						<Button
							type="button"
							onClick={reset}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Registrér en til
						</Button>
						<Button
							variant="secondary"
							type="button"
							onClick={onClose}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Færdig
						</Button>
					</div>
				</div>
			) : (
				<form
					onSubmit={handleSubmit}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.modalBody,
					)}
				>
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Dato
							</span>
							<Input
								disabled={outcome.blocked}
								ref={firstFieldRef}
								type="date"
								value={tripDate}
								onChange={(e) => setTripDate(e.target.value)}
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Formål
							</span>
							<Input
								disabled={outcome.blocked}
								type="text"
								value={purpose}
								onChange={(e) => setPurpose(e.target.value)}
								placeholder="Kundebesøg Aarhus"
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Fra-adresse
							</span>
							<Input
								disabled={outcome.blocked}
								type="text"
								value={fromLocation}
								onChange={(e) => setFromLocation(e.target.value)}
								placeholder="København"
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Til-adresse
							</span>
							<Input
								disabled={outcome.blocked}
								type="text"
								value={toLocation}
								onChange={(e) => setToLocation(e.target.value)}
								placeholder="Aarhus"
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Antal km
							</span>
							<Input
								disabled={outcome.blocked}
								type="number"
								step="0.1"
								min="0"
								value={kilometers}
								onChange={(e) => setKilometers(e.target.value)}
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Køretøj
							</span>
							<Input
								disabled={outcome.blocked}
								type="text"
								value={vehicle}
								onChange={(e) => setVehicle(e.target.value)}
								placeholder="Privat bil"
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Chauffør
							</span>
							<Input
								disabled={outcome.blocked}
								type="text"
								value={driver}
								onChange={(e) => setDriver(e.target.value)}
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Takst (kr/km)
							</span>
							<Input
								disabled={outcome.blocked}
								type="number"
								step="0.01"
								min="0"
								value={ratePerKm}
								onChange={(e) => setRatePerKm(e.target.value)}
								required
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Takst-grundlag
							</span>
							<Input
								disabled={outcome.blocked}
								type="text"
								value={rateBasis}
								onChange={(e) => setRateBasis(e.target.value)}
								placeholder="SKAT 2026, høj sats (op til 20.000 km)"
								required
								xstyle={[cockpitStyles.inputComposition]}
							/>
							<small
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Rentemester ejer ikke skattesatsen — du bekræfter hvilken
								officiel sats du bruger. Slå op på{" "}
								<a
									href="https://skat.dk/erhverv/moms/regler-og-satser/satser-for-erhvervsmaessig-koersel"
									target="_blank"
									rel="noreferrer noopener"
									{...stylex.props(cockpitStyles.aComposition)}
								>
									skat.dk
								</a>
								.
							</small>
						</label>
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.formGridLabel,
							)}
						>
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Takst-kilde (valgfri)
							</span>
							<Input
								disabled={outcome.blocked}
								type="url"
								value={rateSource}
								onChange={(e) => setRateSource(e.target.value)}
								placeholder="https://skat.dk/…"
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
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Noter (valgfri)
							</span>
							<Textarea
								disabled={outcome.blocked}
								value={notes}
								onChange={(e) => setNotes(e.target.value)}
								rows={2}
								xstyle={[cockpitStyles.textareaComposition]}
							/>
						</label>
					</div>

					{amountBasis !== null && amountBasis > 0 && (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
								cockpitStyles.modalBodyP,
								viewStyles.site1,
							)}
						>
							Godtgørelsesgrundlag: {formatKroner(amountBasis, "DKK")} (km ×
							takst). Beløbet er dokumentation — kørselsregisteret bogfører
							aldrig direkte.
						</p>
					)}

					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rowActions,
							viewStyles.site2,
						)}
					>
						<Button
							requiredPermission="company.ledger.post"
							type="submit"
							disabled={outcome.blocked || busy}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							{busy ? "Registrerer…" : "Registrér kørsel"}
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
			)}
		</Dialog>
	);
}

const viewStyles = stylex.create({
	site0: { marginTop: "0.75rem" },
	site1: { marginTop: "0.5rem" },
	site2: { marginTop: "0.75rem" },
});
