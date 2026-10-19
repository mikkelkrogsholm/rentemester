import { useState } from "react";
import { Button, Dialog, Field, Input, Select } from "../components/ui";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatKroner, parseDanishAmount, todayIso } from "../lib/format";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import type { AccrualRegisterInput, AccrualRegisterRow } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
const labels = {
	prepaid_expense: "Forudbetalt omkostning",
	accrued_expense: "Skyldig omkostning",
	deferred_revenue: "Udskudt omsætning",
};
export function registrationPreview(input: AccrualRegisterInput) {
	const cents = Math.round(input.totalAmount * 100),
		n = input.recognitionPeriods;
	if (
		!Number.isSafeInteger(cents) ||
		cents <= 0 ||
		!Number.isInteger(n) ||
		n < 1 ||
		n > 12000 ||
		Math.floor(cents / n) < 1 ||
		!Number.isInteger(input.periodStepMonths) ||
		input.periodStepMonths < 1 ||
		input.periodStepMonths > 1200
	)
		throw new Error(
			"Vælg positivt beløb, 1–12000 perioder og 1–1200 måneder mellem perioderne.",
		);
	const first = new Date(`${input.firstRecognitionDate}T12:00:00Z`);
	if (
		!Number.isFinite(first.getTime()) ||
		first.toISOString().slice(0, 10) !== input.firstRecognitionDate
	)
		throw new Error("Vælg en gyldig første dato.");
	return Array.from({ length: n }, (_, index) => {
		const date = new Date(first);
		date.setUTCDate(1);
		date.setUTCFullYear(
			first.getUTCFullYear(),
			first.getUTCMonth() + index * input.periodStepMonths,
			1,
		);
		const end = new Date(date);
		end.setUTCMonth(end.getUTCMonth() + 1, 0);
		const last = end.getUTCDate();
		date.setUTCDate(Math.min(first.getUTCDate(), last));
		if (date.getUTCFullYear() > 9999)
			throw new Error("Planen ligger uden for kalenderen.");
		return {
			periodIndex: index + 1,
			recognitionDate: date.toISOString().slice(0, 10),
			amount:
				(index === n - 1
					? cents - Math.floor(cents / n) * (n - 1)
					: Math.floor(cents / n)) / 100,
		};
	});
}
export function RegisterAccrualModal({
	slug,
	onClose,
	onSaved,
}: {
	slug: string;
	onClose: () => void;
	onSaved: () => void;
}) {
	const resources = useAsync(
		async (signal) => ({
			chart: await api.accounts(slug, { signal }),
			docs: await api.documents(slug, { signal }),
		}),
		[slug],
	);
	const [kind, setKind] =
			useState<AccrualRegisterRow["accrualType"]>("prepaid_expense"),
		[description, setDescription] = useState(""),
		[amount, setAmount] = useState(""),
		[periods, setPeriods] = useState(""),
		[step, setStep] = useState("1"),
		[first, setFirst] = useState(todayIso()),
		[date, setDate] = useState(todayIso()),
		[balance, setBalance] = useState(""),
		[result, setResult] = useState(""),
		[settlement, setSettlement] = useState(""),
		[document, setDocument] = useState("");
	const [review, setReview] = useState<AccrualRegisterInput | null>(null),
		[error, setError] = useState(""),
		[busy, setBusy] = useState(false);
	const outcome = useMutationOutcome(undefined, "register-accrual");
	const chart = resources.data?.chart.accounts ?? [];
	function accountField(
		label: string,
		value: string,
		set: (v: string) => void,
		type: string,
		normal: string,
	) {
		return (
			<Field label={label}>
				<Select required value={value} onChange={(e) => set(e.target.value)}>
					<option value="">Vælg konto</option>
					{chart
						.filter(
							(a) =>
								a.active !== false &&
								a.type === type &&
								a.normalBalance === normal,
						)
						.map((a) => (
							<option key={a.accountNo} value={a.accountNo}>
								{a.accountNo} · {a.name}
							</option>
						))}
				</Select>
			</Field>
		);
	}
	async function submit() {
		if (!review || outcome.isBlocked()) return;
		setBusy(true);
		setError("");
		try {
			await api.registerAccrual(slug, review);
			onSaved();
		} catch (e) {
			try {
				outcome.reject(e);
			} catch (err) {
				setError(String(err));
			}
		} finally {
			setBusy(false);
		}
	}
	return (
		<Dialog title="Ny periodisering" onClose={onClose} busy={busy}>
			{resources.loading ? (
				<Loading />
			) : resources.error ? (
				<ErrorState message={resources.error} />
			) : review ? (
				<>
					<p>
						{labels[review.accrualType]}: {review.description}. Registrering{" "}
						{review.registrationDate}: {formatKroner(review.totalAmount, "DKK")}
						. Bilag {review.documentId}.
					</p>
					<p>
						Balancekonto {review.balanceAccountNo}, resultatkonto{" "}
						{review.resultAccountNo}, betalingskonto{" "}
						{review.settlementAccountNo}.{" "}
						{review.accrualType === "accrued_expense"
							? "Registreringen debiterer omkostningen og krediterer gælden. Perioderne debiterer gælden og krediterer den valgte betalingskonto."
							: review.accrualType === "prepaid_expense"
								? "Registreringen debiterer aktivet og krediterer betalingskontoen. Perioderne debiterer omkostningen og krediterer aktivet."
								: "Registreringen debiterer betalingskontoen og krediterer gælden. Perioderne debiterer gælden og krediterer omsætningen."}
					</p>
					<Schedule rows={registrationPreview(review)} />
					<p>
						Bekræftelsen bogfører registreringen med bilaget. Perioderne
						bogføres efterfølgende én ad gangen. Bogførte oplysninger bevares.
					</p>
					<Button
						variant="secondary"
						disabled={busy}
						onClick={() => setReview(null)}
					>
						Tilbage
					</Button>
					<Button
						disabled={busy || outcome.blocked}
						onClick={() => void submit()}
					>
						Registrér og bogfør
					</Button>
				</>
			) : (
				<form
					onSubmit={(e) => {
						e.preventDefault();
						setError("");
						try {
							const input: AccrualRegisterInput = {
								accrualType: kind,
								description,
								totalAmount: parseDanishAmount(amount) ?? 0,
								recognitionPeriods: Number(periods),
								periodStepMonths: Number(step),
								firstRecognitionDate: first,
								registrationDate: date,
								balanceAccountNo: balance,
								resultAccountNo: result,
								settlementAccountNo: settlement,
								documentId: Number(document),
							};
							registrationPreview(input);
							if (date > first)
								throw new Error(
									"Registreringsdato må ikke ligge efter første periode.",
								);
							setReview(input);
						} catch (e) {
							setError(String(e));
						}
					}}
				>
					<Field label="Type">
						<Select
							value={kind}
							onChange={(e) => {
								setKind(e.target.value as typeof kind);
								setBalance("");
								setResult("");
							}}
						>
							{Object.entries(labels).map(([value, label]) => (
								<option key={value} value={value}>
									{label}
								</option>
							))}
						</Select>
					</Field>
					<Field label="Beskrivelse">
						<Input
							required
							maxLength={500}
							value={description}
							onChange={(e) => setDescription(e.target.value)}
						/>
					</Field>
					<Field label="Samlet beløb (kr.)">
						<Input
							required
							inputMode="decimal"
							value={amount}
							onChange={(e) => setAmount(e.target.value)}
						/>
					</Field>
					<Field label="Antal perioder">
						<Input
							required
							type="number"
							min={1}
							max={12000}
							value={periods}
							onChange={(e) => setPeriods(e.target.value)}
						/>
					</Field>
					<Field label="Måneder mellem perioder">
						<Input
							required
							type="number"
							min={1}
							max={1200}
							value={step}
							onChange={(e) => setStep(e.target.value)}
						/>
					</Field>
					<Field label="Registreringsdato">
						<Input
							required
							type="date"
							value={date}
							onChange={(e) => setDate(e.target.value)}
						/>
					</Field>
					<Field label="Første periodedato">
						<Input
							required
							type="date"
							value={first}
							onChange={(e) => setFirst(e.target.value)}
						/>
					</Field>
					{accountField(
						"Balancekonto",
						balance,
						setBalance,
						kind === "prepaid_expense" ? "asset" : "liability",
						kind === "prepaid_expense" ? "debit" : "credit",
					)}
					{accountField(
						"Resultatkonto",
						result,
						setResult,
						kind === "deferred_revenue" ? "income" : "expense",
						kind === "deferred_revenue" ? "credit" : "debit",
					)}
					{accountField(
						"Betalingskonto",
						settlement,
						setSettlement,
						"asset",
						"debit",
					)}
					<Field label="Originalt bilag">
						<Select
							required
							value={document}
							onChange={(e) => setDocument(e.target.value)}
						>
							<option value="">Vælg bilag</option>
							{resources.data?.docs.documents.map((d) => (
								<option key={d.id} value={d.id}>
									{d.documentNo ?? d.id} · {d.filename}
								</option>
							))}
						</Select>
					</Field>
					<p>
						Beløbet periodiseres uden automatisk momsberegning. Vælg beløb og
						konti efter bilaget og den regnskabsmæssige behandling.
					</p>
					<Button type="submit">Gennemse periodisering</Button>
				</form>
			)}
			{error && <p role="alert">{error}</p>}
			{outcome.feedback}
			<Button variant="secondary" disabled={busy} onClick={onClose}>
				Luk
			</Button>
		</Dialog>
	);
}
function Schedule({
	rows,
}: {
	rows: Array<{
		periodIndex: number;
		recognitionDate: string;
		amount: number;
		recognized?: boolean;
	}>;
}) {
	return (
		<table>
			<thead>
				<tr>
					<th>Periode</th>
					<th>Dato</th>
					<th>Beløb</th>
					<th>Status</th>
				</tr>
			</thead>
			<tbody>
				{rows.map((p) => (
					<tr key={p.periodIndex}>
						<td>{p.periodIndex}</td>
						<td>{p.recognitionDate}</td>
						<td>{formatKroner(p.amount, "DKK")}</td>
						<td>{p.recognized ? "Bogført" : "Planlagt"}</td>
					</tr>
				))}
			</tbody>
		</table>
	);
}
export function RecognizeAccrualModal({
	slug,
	row,
	onClose,
	onSaved,
}: {
	slug: string;
	row: AccrualRegisterRow;
	onClose: () => void;
	onSaved: () => void;
}) {
	const state = useAsync(
		async (signal) => ({
			plan: await api.accrualSchedule(slug, row.accrualId, { signal }),
			chart:
				row.accrualType === "accrued_expense"
					? await api.accounts(slug, { signal })
					: null,
		}),
		[slug, row.accrualId],
	);
	const [index, setIndex] = useState(""),
		[date, setDate] = useState(""),
		[settlement, setSettlement] = useState(""),
		[review, setReview] = useState(false),
		[error, setError] = useState(""),
		[busy, setBusy] = useState(false);
	const outcome = useMutationOutcome(
		undefined,
		`recognize-accrual-${row.accrualId}`,
	);
	const selected = state.data?.plan.schedule.find(
		(p) => p.periodIndex === Number(index),
	);
	async function submit() {
		if (!selected || outcome.isBlocked()) return;
		setBusy(true);
		try {
			await api.recognizeAccrual(slug, row.accrualId, {
				periodIndex: selected.periodIndex,
				transactionDate: date,
				...(settlement ? { settlementAccountNo: settlement } : {}),
			});
			onSaved();
		} catch (e) {
			try {
				outcome.reject(e);
			} catch (err) {
				setError(String(err));
			}
		} finally {
			setBusy(false);
		}
	}
	return (
		<Dialog
			title={`Bogfør periode: ${row.description}`}
			onClose={onClose}
			busy={busy}
		>
			{state.loading ? (
				<Loading />
			) : state.error ? (
				<ErrorState message={state.error} />
			) : (
				<>
					<Schedule rows={state.data!.plan.schedule} />
					{review ? (
						<>
							<p>
								Periode {index}: {formatKroner(selected!.amount, "DKK")}{" "}
								bogføres {date} med originalt bilag{" "}
								{state.data!.plan.documentId}.{" "}
								{settlement && `Betalingskonto ${settlement}.`}
							</p>
							<Button
								variant="secondary"
								disabled={busy}
								onClick={() => setReview(false)}
							>
								Tilbage
							</Button>
							<Button
								disabled={busy || outcome.blocked}
								onClick={() => void submit()}
							>
								Bekræft og bogfør periode
							</Button>
						</>
					) : (
						<form
							onSubmit={(e) => {
								e.preventDefault();
								setReview(true);
							}}
						>
							<Field label="Periode">
								<Select
									required
									value={index}
									onChange={(e) => {
										setIndex(e.target.value);
										setDate(
											state.data!.plan.schedule.find(
												(p) => p.periodIndex === Number(e.target.value),
											)?.recognitionDate ?? "",
										);
									}}
								>
									<option value="">Vælg periode</option>
									{state
										.data!.plan.schedule.filter((p) => !p.recognized)
										.map((p) => (
											<option key={p.periodIndex} value={p.periodIndex}>
												{p.periodIndex} · {p.recognitionDate} ·{" "}
												{formatKroner(p.amount, "DKK")}
											</option>
										))}
								</Select>
							</Field>
							<Field label="Bogføringsdato">
								<Input
									required
									type="date"
									min={selected?.recognitionDate}
									value={date}
									onChange={(e) => setDate(e.target.value)}
								/>
							</Field>
							{state.data?.chart && (
								<Field label="Betalingskonto">
									<Select
										required
										value={settlement}
										onChange={(e) => setSettlement(e.target.value)}
									>
										<option value="">Vælg konto</option>
										{state.data.chart.accounts
											.filter(
												(a) =>
													a.type === "asset" &&
													a.normalBalance === "debit" &&
													a.active !== false,
											)
											.map((a) => (
												<option key={a.accountNo} value={a.accountNo}>
													{a.accountNo} · {a.name}
												</option>
											))}
									</Select>
								</Field>
							)}
							<Button type="submit">Gennemse bogføring</Button>
						</form>
					)}
				</>
			)}
			{error && <p role="alert">{error}</p>}
			{outcome.feedback}
			<Button variant="secondary" disabled={busy} onClick={onClose}>
				Luk
			</Button>
		</Dialog>
	);
}
