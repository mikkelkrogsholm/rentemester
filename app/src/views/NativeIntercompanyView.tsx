import { NativeEliminationsPanel } from "./NativeEliminationsPanel";
import { useState, type FormEvent } from "react";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, PageHeader, Select, Textarea } from "../components/ui";
import { companiesApi } from "../lib/api/companies";
import { intercompanyApi } from "../lib/api/intercompany";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";

/** Explicit structure and exact reciprocal evidence; no consolidation or inferred accounting. */
export function NativeIntercompanyView() {
	const companies = useAsync(
		(signal) => companiesApi.companies({ signal }),
		[],
	);
	const [anchor, setAnchor] = useState("");
	const [asOf, setAsOf] = useState(() =>
		new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" }),
	);
	const [action, setAction] = useState<
		"manifest" | "propose" | "approve" | "revoke"
	>("manifest");
	const [body, setBody] = useState("");
	const [confirmed, setConfirmed] = useState(false);
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState<string>();
	const current = useAsync(
		(signal) =>
			anchor
				? intercompanyApi.current(anchor, { signal })
				: Promise.resolve(null),
		[anchor],
	);
	const report = useAsync(
		(signal) => intercompanyApi.reconciliation(asOf, { signal }),
		[asOf],
	);
	const refresh = () => {
		current.reload();
		report.reload();
	};
	const outcome = useMutationOutcome(refresh, "intercompany");
	async function submit(e: FormEvent) {
		e.preventDefault();
		if (!anchor || !confirmed || busy || outcome.isBlocked()) return;
		setBusy(true);
		setMessage(undefined);
		try {
			const value: unknown = JSON.parse(body);
			if (!value || Array.isArray(value) || typeof value !== "object")
				throw Error("Indtast et JSON-objekt.");
			await intercompanyApi
				.mutate(anchor, action, value as Record<string, unknown>)
				.catch(outcome.reject);
			setConfirmed(false);
			setMessage("Registreret. Aktuel struktur og afstemning genlæses.");
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
		<section className="page">
			<PageHeader title="Mellemregningsafstemning" />
			<p>
				Vælg eksplicitte medlemskaber og kontomappings. Selskabernes posteringer
				ændres aldrig her. Kontrollér modstående saldi og kildeposteringer, og
				brug rapportpanelet til den eksplicitte koncernrapport.
			</p>
			<label>
				Pr. dato
				<Input
					aria-label="Afstemning pr. dato"
					type="date"
					required
					value={asOf}
					onChange={(e) => setAsOf(e.target.value)}
				/>
			</label>
			<Button variant="secondary" onClick={refresh}>
				Opdater afstemning
			</Button>
			{report.loading && !report.data && <Loading label="Henter afstemning…" />}
			{report.error && (
				<ErrorState message={report.error} onRetry={report.reload} />
			)}
			{report.data && !report.error && (
				<>
					<p>{report.data.rows.length} synlige aktive mappings</p>
					{report.data.rows.map((r) => (
						<article className="card" key={r.mappingId}>
							<h3>{r.mappingId}</h3>
							<p>
								Status: {r.status} · {r.reason}
							</p>
							{r.left && r.right && (
								<p>
									{r.left.companySlug}: {r.left.balance} {r.left.currency} ·{" "}
									{r.right.companySlug}: {r.right.balance} {r.right.currency}
								</p>
							)}
							{r.difference !== undefined && (
								<p>Eksakt difference: {r.difference}</p>
							)}
							{r.blockers.map((b) => (
								<p key={b}>{b}</p>
							))}
							<details>
								<summary>Posteringer og kildebeviser</summary>
								<pre>{JSON.stringify(r, null, 2)}</pre>
							</details>
						</article>
					))}
				</>
			)}
			<section className="card">
				<h3>Struktur og godkendte mappings</h3>
				<p>
					Ændringer kræver aktuel administratoradgang til alle berørte
					selskaber. En anden person skal godkende mappingens eksakte hash.
				</p>
				{companies.error && (
					<ErrorState message={companies.error} onRetry={companies.reload} />
				)}
				<label>
					Virksomhed til adgangskontrol
					<Select
						aria-label="Intercompany virksomhed"
						value={anchor}
						onChange={(e) => {
							setAnchor(e.target.value);
							setConfirmed(false);
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
				{current.error && (
					<ErrorState message={current.error} onRetry={current.reload} />
				)}
				<details>
					<summary>Aktuelt manifest, mappings og hashes</summary>
					<pre>
						{JSON.stringify(current.error ? null : current.data, null, 2)}
					</pre>
				</details>
				{outcome.feedback}
				<form
					aria-label="Intercompany ændring"
					onSubmit={(e) => void submit(e)}
				>
					<label>
						Handling
						<Select
							aria-label="Intercompany handling"
							value={action}
							disabled={busy || outcome.blocked}
							onChange={(e) => {
								setAction(e.target.value as typeof action);
								setBody("");
								setConfirmed(false);
							}}
						>
							<option value="manifest">Registrér V1-gruppestruktur</option>
							<option value="propose">Foreslå kontomapping</option>
							<option value="approve">Godkend eksakt mapping</option>
							<option value="revoke">Tilbagekald eksakt mapping</option>
						</Select>
					</label>
					<p>
						Manifest: version 1 og groups med id, name, memberships og
						ownership. Mapping: id, groupId, begge selskaber, kontonumre,
						positioner, kildehenvisninger og gyldighed.
						Godkendelse/tilbagekaldelse: mappingId og mappingHash fra den
						aktuelle læsning.
					</p>
					<label>
						Eksplicitte data (JSON)
						<Textarea
							aria-label="Intercompany JSON"
							rows={10}
							value={body}
							disabled={busy || outcome.blocked}
							onChange={(e) => {
								setBody(e.target.value);
								setConfirmed(false);
							}}
							required
						/>
					</label>
					<label>
						<Input
							aria-label="Bekræft intercompany"
							type="checkbox"
							checked={confirmed}
							disabled={busy || outcome.blocked}
							onChange={(e) => setConfirmed(e.target.checked)}
						/>{" "}
						Jeg har gennemgået selskaber, kilder, konti, datoer og den konkrete
						handling.
					</label>
					<Button
						type="submit"
						disabled={!anchor || !body || !confirmed || busy || outcome.blocked}
					>
						Registrér intercompany-handling
					</Button>
				</form>
				{message && (
					<Banner
						kind={message.startsWith("Registreret") ? "success" : "error"}
					>
						{message}
					</Banner>
				)}
			</section>
			<NativeEliminationsPanel
				anchor={anchor}
				asOf={asOf}
				mappings={current.error ? [] : (current.data?.mappings ?? [])}
			/>
		</section>
	);
}
