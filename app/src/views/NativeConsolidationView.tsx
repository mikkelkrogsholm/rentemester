import { useState, type FormEvent } from "react";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, Select, Textarea } from "../components/ui";
import { companiesApi } from "../lib/api/companies";
import { consolidationApi } from "../lib/api/consolidation";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
/** Reviewed reporting charts; all financial arithmetic and eligibility remain server-owned. */
export function NativeConsolidationView() {
	const companies = useAsync(
		(signal) => companiesApi.companies({ signal }),
		[],
	);
	const [anchor, setAnchor] = useState("");
	const [asOf, setAsOf] = useState("");
	const [from, setFrom] = useState("");
	const [profileId, setProfileId] = useState("");
	const [query, setQuery] = useState<{
		profileId: string;
		from: string;
		asOf: string;
	} | null>(null);
	const current = useAsync(
		(signal) =>
			anchor && asOf
				? consolidationApi.current(anchor, asOf, { signal })
				: Promise.resolve(null),
		[anchor, asOf],
	);
	const report = useAsync(
		(signal) =>
			query
				? consolidationApi.report(query.profileId, query.from, query.asOf, {
						signal,
					})
				: Promise.resolve(null),
		[query],
	);
	const [action, setAction] = useState<"propose" | "approve" | "revoke">(
		"propose",
	);
	const [body, setBody] = useState("");
	const [confirmed, setConfirmed] = useState(false);
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState<string>();
	const refresh = () => {
		current.reload();
		report.reload();
	};
	const outcome = useMutationOutcome(refresh, "consolidation");
	async function submit(e: FormEvent) {
		e.preventDefault();
		if (!anchor || !confirmed || busy || outcome.isBlocked()) return;
		setBusy(true);
		setMessage(undefined);
		try {
			const value: unknown = JSON.parse(body);
			if (!value || Array.isArray(value) || typeof value !== "object")
				throw Error("Indtast et JSON-objekt.");
			await consolidationApi
				.mutate(anchor, action, value as Record<string, unknown>)
				.catch(outcome.reject);
			setConfirmed(false);
			setMessage("Registreret. Aktuelle profiler genlæses.");
			refresh();
		} catch (e) {
			setMessage(
				e instanceof Error ? e.message : "Handlingen kunne ikke bekræftes.",
			);
		} finally {
			setBusy(false);
		}
	}
	return (
		<section className="card" aria-label="Konsolideret rapport">
			<h2>Konsolideret rapport</h2>
			<p>
				Et læselag over V1-gruppestrukturen med eksplicit kontomapping og
				godkendte elimineringer. Selskabernes posteringer ændres ikke. Alle
				beløb og kildebeviser beregnes af serveren.
			</p>
			{companies.error && (
				<ErrorState message={companies.error} onRetry={companies.reload} />
			)}
			<label>
				Virksomhed til adgangskontrol
				<Select
					aria-label="Konsolidering virksomhed"
					value={anchor}
					onChange={(e) => {
						setAnchor(e.target.value);
						setConfirmed(false);
						setQuery(null);
					}}
				>
					<option value="">Vælg virksomhed</option>
					{companies.data
						?.filter((c) => !c.archived)
						.map((c) => (
							<option value={c.slug} key={c.slug}>
								{c.name}
							</option>
						))}
				</Select>
			</label>
			<label>
				Rapport pr. dato
				<Input
					aria-label="Konsolidering pr. dato"
					type="date"
					value={asOf}
					onChange={(e) => {
						setAsOf(e.target.value);
						setConfirmed(false);
					}}
				/>
			</label>
			{current.loading && <Loading label="Henter rapportprofiler…" />}
			{current.error && (
				<ErrorState message={current.error} onRetry={current.reload} />
			)}
			<details>
				<summary>Aktuelle rapportprofiler og eksakte hashes</summary>
				<pre>
					{JSON.stringify(current.error ? null : current.data, null, 2)}
				</pre>
			</details>
			<form
				aria-label="Hent konsolideret rapport"
				onSubmit={(e) => {
					e.preventDefault();
					setQuery({ profileId, from, asOf });
				}}
			>
				<label>
					Godkendt rapportprofil
					<Select
						aria-label="Konsolidering rapportprofil"
						value={profileId}
						onChange={(e) => setProfileId(e.target.value)}
					>
						<option value="">Vælg rapportprofil</option>
						{!current.error &&
							current.data?.availableProfiles.map((p) => (
								<option value={p.id} key={p.id}>
									{p.id} · {p.currency}
								</option>
							))}
					</Select>
				</label>
				<label>
					Fra dato
					<Input
						aria-label="Konsolidering fra dato"
						type="date"
						required
						value={from}
						onChange={(e) => setFrom(e.target.value)}
					/>
				</label>
				<Button
					type="submit"
					disabled={!anchor || !profileId || !from || !asOf || from > asOf}
				>
					Hent konsolideret rapport
				</Button>
			</form>
			{report.loading && <Loading label="Beregner rapport…" />}
			{report.error && (
				<ErrorState message={report.error} onRetry={report.reload} />
			)}
			{report.data && !report.error && (
				<article aria-label="Konsolideringsresultat">
					<p>
						Status: {report.data.status} · {report.data.period.from} –{" "}
						{report.data.period.to} {report.data.currency}
					</p>
					{report.data.blockers.map((b) => (
						<Banner kind="warning" key={b}>
							{b}
						</Banner>
					))}
					{report.data.consolidatedFigures && (
						<table>
							<thead>
								<tr>
									<th>Rapportlinje</th>
									<th>Selskabssum</th>
									<th>Eliminering</th>
									<th>Konsolideret</th>
								</tr>
							</thead>
							<tbody>
								{report.data.consolidatedFigures.map((l) => (
									<tr key={l.lineId} data-line-id={l.lineId}>
										<th>{l.label}</th>
										<td>{l.rawCompanySum}</td>
										<td>{l.eliminationAdjustment}</td>
										<td>{l.consolidatedAmount}</td>
									</tr>
								))}
							</tbody>
						</table>
					)}
					<details>
						<summary>Rapportens fulde kildebeviser</summary>
						<pre>{JSON.stringify(report.data, null, 2)}</pre>
					</details>
				</article>
			)}
			<h3>Rapportprofil og uafhængig godkendelse</h3>
			<p>
				Forslag: profile med version, id, groupId, currency, reportingLines,
				accountMappings, evidenceRefs og gyldighed. Godkendelse: profileId og
				profileHash fra den aktuelle læsning; en anden person skal godkende.
				Tilbagekaldelse: profileId. Der kræves aktuel administratoradgang til
				alle gruppens konfigurerede selskaber.
			</p>
			{outcome.feedback}
			<form
				aria-label="Konsolideringsprofil ændring"
				onSubmit={(e) => void submit(e)}
			>
				<label>
					Handling
					<Select
						aria-label="Konsolidering handling"
						value={action}
						disabled={busy || outcome.blocked}
						onChange={(e) => {
							setAction(e.target.value as typeof action);
							setBody("");
							setConfirmed(false);
						}}
					>
						<option value="propose">Foreslå rapportprofil</option>
						<option value="approve">Godkend eksakt rapportprofil</option>
						<option value="revoke">Tilbagekald rapportprofil</option>
					</Select>
				</label>
				<label>
					Eksplicitte profildata (JSON)
					<Textarea
						aria-label="Konsolidering JSON"
						rows={12}
						required
						disabled={busy || outcome.blocked}
						value={body}
						onChange={(e) => {
							setBody(e.target.value);
							setConfirmed(false);
						}}
					/>
				</label>
				<label>
					<Input
						aria-label="Bekræft konsolideringsprofil"
						type="checkbox"
						checked={confirmed}
						disabled={busy || outcome.blocked}
						onChange={(e) => setConfirmed(e.target.checked)}
					/>{" "}
					Jeg har gennemgået selskaber, konti, kilder, datoer og den konkrete
					handling.
				</label>
				<Button
					type="submit"
					disabled={!anchor || !confirmed || !body || busy || outcome.blocked}
				>
					Registrér rapportprofil-handling
				</Button>
			</form>
			{message && (
				<Banner kind={message.startsWith("Registreret") ? "success" : "error"}>
					{message}
				</Banner>
			)}
		</section>
	);
}
