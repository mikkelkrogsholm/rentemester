import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState, type FormEvent } from "react";
import { Banner, ErrorState } from "../components/Feedback";
import { Button, Input, Select, Textarea } from "../components/ui";
import {
	eliminationsApi,
	type EliminationInput,
} from "../lib/api/eliminations";
import type { MappingState } from "../lib/api/intercompany";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
const labels = {
	proposed: "Foreslået",
	approved: "Godkendt",
	rejected: "Afvist",
	applied: "Anvendt",
	reversed: "Tilbageført",
};
const decimal = (ore: string) => {
	const s = ore.padStart(3, "0");
	return `${s.slice(0, -2)}.${s.slice(-2)}`;
};
/** Explicit reviewed report adjustments. Company journals remain immutable and untouched. */
export function NativeEliminationsPanel({
	anchor,
	asOf,
	mappings,
}: {
	anchor: string;
	asOf: string;
	mappings: MappingState[];
}) {
	const current = useAsync(
		(signal) =>
			anchor
				? eliminationsApi.current(anchor, { signal })
				: Promise.resolve(null),
		[anchor],
	);
	const [id, setId] = useState(""),
		[mappingId, setMappingId] = useState(""),
		[refs, setRefs] = useState(""),
		[selected, setSelected] = useState(""),
		[confirmed, setConfirmed] = useState(false),
		[actionConfirmed, setActionConfirmed] = useState(false),
		[busy, setBusy] = useState(false),
		[message, setMessage] = useState<string>();
	const [failed, setFailed] = useState(false);
	const [plan, setPlan] = useState<{
		input: EliminationInput;
		payloadHash: string;
		payload: Awaited<
			ReturnType<typeof eliminationsApi.plan>
		>["plan"]["payload"];
	}>();
	const input: EliminationInput = {
		id,
		mappingId,
		asOf,
		evidenceRefs: refs
			.split("\n")
			.map((s) => s.trim())
			.filter(Boolean),
	};
	const validPlan = Boolean(
		plan && JSON.stringify(input) === JSON.stringify(plan.input),
	);
	const state = current.error
		? undefined
		: current.data?.rows.find((s) => s.payload.id === selected);
	const outcome = useMutationOutcome(current.reload, "eliminations");
	async function preview(e: FormEvent) {
		e.preventDefault();
		if (!anchor || busy) return;
		setBusy(true);
		setPlan(undefined);
		setConfirmed(false);
		setMessage(undefined);
		setFailed(false);
		try {
			const result = await eliminationsApi.plan(anchor, input);
			setPlan({ ...result.plan, input });
		} catch (e) {
			setFailed(true);
			setMessage(e instanceof Error ? e.message : "Planen kunne ikke læses.");
		} finally {
			setBusy(false);
		}
	}
	async function propose() {
		if (!plan || !validPlan || !confirmed || outcome.isBlocked() || busy)
			return;
		setBusy(true);
		setMessage(undefined);
		setFailed(false);
		try {
			await eliminationsApi
				.propose(anchor, plan.input, plan.payloadHash)
				.catch(outcome.reject);
			setSelected(plan.payload.id);
			setPlan(undefined);
			setConfirmed(false);
			setActionConfirmed(false);
			setMessage(
				"Forslaget er registreret. En anden person skal godkende og anvende det.",
			);
			current.reload();
		} catch (e) {
			setFailed(true);
			setMessage(
				e instanceof Error ? e.message : "Forslaget kunne ikke bekræftes.",
			);
		} finally {
			setBusy(false);
		}
	}
	async function act(action: "approve" | "reject" | "apply" | "reverse") {
		if (!state || !actionConfirmed || busy || outcome.isBlocked()) return;
		setBusy(true);
		setMessage(undefined);
		setFailed(false);
		try {
			await eliminationsApi.act(anchor, action, state).catch(outcome.reject);
			setActionConfirmed(false);
			setMessage(
				"Elimineringens aktuelle tilstand genlæses. Ingen selskabspostering er ændret.",
			);
			current.reload();
		} catch (e) {
			setFailed(true);
			setMessage(
				e instanceof Error ? e.message : "Handlingen kunne ikke bekræftes.",
			);
		} finally {
			setBusy(false);
		}
	}
	return (
		<section {...stylex.props(cockpitStyles.card)} aria-label="Balanceelimineringer">
			<h2>Balanceelimineringer</h2>
			<p>
				Kun eksakt afstemte, positive mellemregninger kan elimineres.
				Elimineringer gælder den konkrete rapportdato og ændrer ikke
				selskabernes posteringer. Et ændret kildegrundlag kræver et nyt forslag.
			</p>
			{!anchor && <p>Vælg først en virksomhed til adgangskontrol ovenfor.</p>}
			{outcome.feedback}
			<form
				aria-label="Planlæg balanceeliminering"
				onSubmit={(e) => void preview(e)}
			>
				<label>
					Eliminerings-id
					<Input
						aria-label="Eliminerings-id"
						value={id}
						required
						onChange={(e) => {
							setId(e.target.value);
							setConfirmed(false);
						}}
						disabled={busy || outcome.blocked}
					/>
				</label>
				<label>
					Godkendt kontomapping
					<Select
						aria-label="Eliminering mapping"
						value={mappingId}
						required
						onChange={(e) => {
							setMappingId(e.target.value);
							setConfirmed(false);
						}}
						disabled={busy || outcome.blocked}
					>
						<option value="">Vælg mapping</option>
						{mappings
							.filter((s) => s.status === "approved")
							.map((s) => (
								<option value={s.mapping.id} key={s.mapping.id}>
									{s.mapping.id} · {s.mapping.leftCompanySlug} ↔{" "}
									{s.mapping.rightCompanySlug}
								</option>
							))}
					</Select>
				</label>
				<p>Rapportdato: {asOf}</p>
				<label>
					Kildereferencer, én pr. linje
					<Textarea
						aria-label="Eliminering kildereferencer"
						rows={3}
						value={refs}
						required
						onChange={(e) => {
							setRefs(e.target.value);
							setConfirmed(false);
						}}
						disabled={busy || outcome.blocked}
					/>
				</label>
				<Button
					variant="secondary"
					type="submit"
					disabled={
						!anchor || !mappingId || !id || !refs || busy || outcome.blocked
					}
				>
					Vis elimineringsplan
				</Button>
			</form>
			{plan && validPlan && (
				<div>
					<h3>Gennemgå planen</h3>
					<p>
						Beløb: {decimal(plan.payload.amountOre)} {plan.payload.currency} ·{" "}
						{plan.payload.asOf}
					</p>
					{[plan.payload.left, plan.payload.right].map((side) => (
						<p key={side.companySlug}>
							{side.companySlug} · {side.accountNos.join(", ")} · Debet{" "}
							{decimal(side.debitOre ?? "0")} / Kredit{" "}
							{decimal(side.creditOre ?? "0")} {plan.payload.currency}
						</p>
					))}
					<details>
						<summary>Eksakte kilder og planhash</summary>
						<pre>{JSON.stringify(plan.payload, null, 2)}</pre>
						<code>{plan.payloadHash}</code>
					</details>
					<label>
						<Input
							aria-label="Bekræft elimineringsforslag"
							type="checkbox"
							checked={confirmed}
							onChange={(e) => setConfirmed(e.target.checked)}
							disabled={busy || outcome.blocked}
						/>{" "}
						Jeg har gennemgået selskaber, dato, konti, beløb og kilder.
					</label>
					<Button
						onClick={() => void propose()}
						disabled={!confirmed || busy || outcome.blocked}
					>
						Foreslå eliminering
					</Button>
				</div>
			)}
			<h3>Forslag og anvendte beslutninger</h3>
			<Button variant="secondary" onClick={current.reload}>
				Opdater elimineringer
			</Button>
			{current.error && (
				<ErrorState message={current.error} onRetry={current.reload} />
			)}
			<label>
				Eliminering
				<Select
					aria-label="Vælg eliminering"
					value={selected}
					onChange={(e) => {
						setSelected(e.target.value);
						setActionConfirmed(false);
					}}
				>
					<option value="">Vælg synlig eliminering</option>
					{!current.error &&
						current.data?.rows.map((s) => (
							<option value={s.payload.id} key={s.payload.id}>
								{s.payload.id} · {labels[s.status]}
							</option>
						))}
				</Select>
			</label>
			{state && (
				<div>
					<p>Status: {labels[state.status]}</p>
					<p>
						{decimal(state.payload.amountOre)} {state.payload.currency} ·{" "}
						{state.payload.asOf} · {state.payload.left.companySlug} ↔{" "}
						{state.payload.right.companySlug}
					</p>
					<details>
						<summary>Uændret beslutningsgrundlag og beviser</summary>
						<pre>{JSON.stringify(state, null, 2)}</pre>
					</details>
					<p>
						Godkendelse og anvendelse kræver en anden person end
						forslagsstilleren. Ved forældede kilder: tilbagefør en anvendt
						beslutning og planlæg et nyt forslag.
					</p>
					<label>
						<Input
							aria-label="Bekræft elimineringsbeslutning"
							type="checkbox"
							checked={actionConfirmed}
							disabled={busy || outcome.blocked}
							onChange={(e) => setActionConfirmed(e.target.checked)}
						/>{" "}
						Jeg har gennemgået den valgte beslutning og dens eksakte hash.
					</label>
					{state.status === "proposed" && (
						<>
							<Button
								disabled={!actionConfirmed || busy || outcome.blocked}
								onClick={() => void act("approve")}
							>
								Godkend eliminering
							</Button>
							<Button
								variant="secondary"
								disabled={!actionConfirmed || busy || outcome.blocked}
								onClick={() => void act("reject")}
							>
								Afvis eliminering
							</Button>
						</>
					)}
					{state.status === "approved" && (
						<Button
							disabled={!actionConfirmed || busy || outcome.blocked}
							onClick={() => void act("apply")}
						>
							Anvend eliminering
						</Button>
					)}
					{state.status === "applied" && (
						<Button
							variant="secondary"
							disabled={!actionConfirmed || busy || outcome.blocked}
							onClick={() => void act("reverse")}
						>
							Tilbagefør eliminering
						</Button>
					)}
				</div>
			)}
			{message && (
				<Banner kind={failed ? "error" : "success"}>{message}</Banner>
			)}
		</section>
	);
}
