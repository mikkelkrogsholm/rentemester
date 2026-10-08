import * as stylex from "@stylexjs/stylex";
import { type FormEvent, useState } from "react";
import { useParams } from "react-router-dom";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, PageHeader, Select, Textarea } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { workspaceRegistryApi } from "../lib/api/workspace-registry";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

type Action = {
	permission:
		| "company.master-data"
		| "company.review"
		| "company.knowledge.manage"
		| "company.ownership.manage";
	label: string;
	help: string;
	initial: string;
	run: (body: Record<string, unknown>) => Promise<unknown>;
};
const parse = (value: string): Record<string, unknown> => {
	const result = JSON.parse(value);
	if (!result || Array.isArray(result) || typeof result !== "object")
		throw new Error("Indtast et JSON-objekt.");
	return result as Record<string, unknown>;
};

/** Thin Cockpit adapter: the server remains the single role/business gate. */
function ActionForm({
	action,
	onDone,
}: {
	action: Action;
	onDone: () => void;
}) {
	const [payload, setPayload] = useState(action.initial);
	const [savedPayload, setSavedPayload] = useState(action.initial);
	const [confirmed, setConfirmed] = useState(false);
	const [busy, setBusy] = useState(false);
	const [result, setResult] = useState<string>();
	const outcome = useMutationOutcome(onDone);
	useUnsavedChanges(payload !== savedPayload);
	async function submit(event: FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		setResult(undefined);
		try {
			setBusy(true);
			await outcome.run(() => action.run(parse(payload)));
			setSavedPayload(payload);
			setResult("Handlingen er registreret i revisionssporet.");
			onDone();
		} catch (error) {
			setResult(
				error instanceof Error
					? error.message
					: "Handlingen kunne ikke udføres.",
			);
		} finally {
			setBusy(false);
		}
	}
	return (
		<form
			onSubmit={(event) => void submit(event)}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.modalBody,
			)}
		>
			{outcome.feedback}
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.modalBodyP,
				)}
			>
				{action.help} Serveren kontrollerer din aktuelle rolle og adgang.
			</p>
			<label
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalField,
				)}
			>
				JSON-data
				<Textarea
					disabled={outcome.blocked}
					aria-label={`${action.label} JSON`}
					value={payload}
					onChange={(event) => setPayload(event.target.value)}
					rows={5}
					xstyle={[cockpitStyles.modalFieldTextareaFocusComposition]}
				/>
			</label>
			<label
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalCheckbox,
				)}
			>
				<Input
					disabled={outcome.blocked}
					type="checkbox"
					checked={confirmed}
					onChange={(event) => setConfirmed(event.target.checked)}
					xstyle={[cockpitStyles.modalCheckboxInputComposition2]}
				/>{" "}
				Jeg bekræfter denne auditerede ændring
			</label>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalActions,
				)}
			>
				<Button
					requiredPermission={action.permission}
					variant="secondary"
					type="submit"
					disabled={outcome.blocked || !confirmed || busy}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					{busy ? "Arbejder…" : action.label}
				</Button>
			</div>
			{result && (
				<Banner kind={result.includes("registreret") ? "success" : "error"}>
					{result}
				</Banner>
			)}
		</form>
	);
}

function RecordIngest({ slug, onDone }: { slug: string; onDone: () => void }) {
	const [file, setFile] = useState<File>();
	const [type, setType] = useState("other");
	const [confirmed, setConfirmed] = useState(false);
	const [busy, setBusy] = useState(false);
	const [result, setResult] = useState<string>();
	const outcome = useMutationOutcome(onDone);
	useUnsavedChanges(Boolean(file) && !result?.includes("indlæst"));
	async function submit(event: FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		if (!file) {
			setResult("Vælg en fil først.");
			return;
		}
		if (file.size > 8 * 1024 * 1024) {
			setResult(
				"Filen er for stor til Cockpit; brug CLI/MCP for den kontrollerede import.",
			);
			return;
		}
		try {
			setBusy(true);
			const bytesBase64 = await new Promise<string>((resolve, reject) => {
				const reader = new FileReader();
				reader.onerror = () => reject(new Error("Filen kunne ikke læses."));
				reader.onload = () =>
					resolve(String(reader.result).split(",")[1] ?? "");
				reader.readAsDataURL(file);
			});
			await outcome.run(() =>
				workspaceRegistryApi.recordIngest(slug, {
					type,
					bytesBase64,
					filename: file.name,
					source: "cockpit_upload",
					receivedAt: new Date().toISOString(),
					uploader: "cockpit",
					sensitivity: "normal",
					links: [{ type: "company", id: slug }],
				}),
			);
			setResult("Den immutable record er indlæst.");
			onDone();
		} catch (error) {
			setResult(
				error instanceof Error ? error.message : "Record kunne ikke indlæses.",
			);
		} finally {
			setBusy(false);
		}
	}
	return (
		<form
			onSubmit={(event) => void submit(event)}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.modalBody,
			)}
		>
			{outcome.feedback}
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.modalBodyP,
				)}
			>
				Filen gemmes som immutable original med SHA-256. Upload aldrig
				credentials eller hemmeligheder.
			</p>
			<label
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalField,
				)}
			>
				Fil
				<Input
					disabled={outcome.blocked}
					aria-label="Corporate record fil"
					type="file"
					onChange={(event) => {
						setFile(event.target.files?.[0]);
						setResult(undefined);
					}}
					xstyle={[cockpitStyles.modalFieldInputFocusComposition]}
				/>
			</label>
			<label
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalField,
				)}
			>
				Type
				<Select
					disabled={outcome.blocked}
					value={type}
					onChange={(event) => setType(event.target.value)}
					xstyle={[cockpitStyles.modalFieldSelectFocusComposition]}
				>
					<option
						value="other"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Andet governance-materiale
					</option>
					<option
						value="articles"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Vedtægter
					</option>
					<option
						value="registration"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Registrering
					</option>
					<option
						value="board_resolution"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bestyrelsesbeslutning
					</option>
					<option
						value="ownership_register"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Ejerbog
					</option>
				</Select>
			</label>
			<label
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalCheckbox,
				)}
			>
				<Input
					disabled={outcome.blocked}
					type="checkbox"
					checked={confirmed}
					onChange={(event) => setConfirmed(event.target.checked)}
					xstyle={[cockpitStyles.modalCheckboxInputComposition2]}
				/>{" "}
				Jeg bekræfter upload af dette governance-dokument
			</label>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.modalActions,
				)}
			>
				<Button
					requiredPermission="company.master-data"
					variant="secondary"
					type="submit"
					disabled={outcome.blocked || !file || !confirmed || busy}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					{busy ? "Indlæser…" : "Indlæs immutable record"}
				</Button>
			</div>
			{result && (
				<Banner kind={result.includes("indlæst") ? "success" : "error"}>
					{result}
				</Banner>
			)}
		</form>
	);
}

export function WorkspaceRegistryView() {
	const { slug = "" } = useParams();
	const [asOf, setAsOf] = useState(() => new Date().toISOString().slice(0, 10));
	const parties = useAsync(
		(signal) => workspaceRegistryApi.workspaceParties(slug, { signal }),
		[slug],
	);
	const records = useAsync(
		(signal) => workspaceRegistryApi.corporateRecords(slug, { signal }),
		[slug],
	);
	const knowledge = useAsync(
		(signal) => workspaceRegistryApi.companyKnowledge(slug, { signal }),
		[slug],
	);
	const ownership = useAsync(
		(signal) => workspaceRegistryApi.ownership(slug, asOf, { signal }),
		[slug, asOf],
	);
	const history = useAsync(
		(signal) => workspaceRegistryApi.ownershipHistory(slug, { signal }),
		[slug],
	);
	const reload = () => {
		parties.reload();
		records.reload();
		knowledge.reload();
		ownership.reload();
		history.reload();
	};
	if (
		(parties.loading || records.loading || knowledge.loading) &&
		!parties.data &&
		!records.data
	)
		return <Loading label="Henter workspace-register…" />;
	if (parties.error || records.error || knowledge.error)
		return (
			<ErrorState
				message={
					parties.error ??
					records.error ??
					knowledge.error ??
					"Register unavailable"
				}
				onRetry={reload}
			/>
		);
	const context = knowledge.data?.context;
	const partyActions: Action[] = [
		{
			permission: "company.master-data",
			label: "Opret part",
			run: (body) => workspaceRegistryApi.partyCreate(slug, body),
			initial:
				'{"name":"","kind":"organization","role":"vendor","source":"manual","observedAt":"2026-01-01","reviewAssertion":""}',
			help: "Opretter en canonical part og én lokal virksomhedsrolle.",
		},
		{
			permission: "company.master-data",
			label: "Knyt rolle",
			run: (body) =>
				workspaceRegistryApi.partyRole(slug, String(body.partyId ?? ""), body),
			initial: '{"partyId":"party-id","role":"vendor"}',
			help: "Knytter en rolle kun til denne virksomhed.",
		},
		{
			permission: "company.review",
			label: "Foreslå merge",
			run: (body) => workspaceRegistryApi.partyMerge(slug, "propose", body),
			initial: '{"fromPartyId":"","intoPartyId":"","reviewAssertion":""}',
			help: "Foreslår en reviewet merge; den udføres aldrig automatisk.",
		},
		{
			permission: "company.review",
			label: "Godkend supersession",
			run: (body) => workspaceRegistryApi.partyMerge(slug, "approve", body),
			initial: '{"fromPartyId":"","proposalHash":""}',
			help: "Godkender præcis den reviewede proposal-hash append-only.",
		},
	];
	const recordActions: Action[] = [
		{
			permission: "company.master-data",
			label: "Knyt record",
			run: (body) =>
				workspaceRegistryApi.recordAction(
					slug,
					String(body.recordId ?? ""),
					"link",
					body,
				),
			initial: `{"recordId":"","type":"company","id":"${slug}"}`,
			help: "Knytter eksisterende immutable evidence med en typed reference.",
		},
		{
			permission: "company.master-data",
			label: "Berig metadata",
			run: (body) =>
				workspaceRegistryApi.recordAction(
					slug,
					String(body.recordId ?? ""),
					"enrich",
					body,
				),
			initial: '{"recordId":"","assertion":""}',
			help: "Tilføjer provenance uden at ændre filens bytes eller hash.",
		},
		{
			permission: "company.master-data",
			label: "Supersedér record",
			run: (body) =>
				workspaceRegistryApi.recordAction(
					slug,
					String(body.recordId ?? ""),
					"supersede",
					body,
				),
			initial: '{"recordId":"","replacementRecordId":"","reason":""}',
			help: "Opretter en append-only korrektionskæde; ingen original overskrives.",
		},
	];
	const knowledgeActions: Action[] = [
		{
			permission: "company.knowledge.manage",
			label: "Foreslå viden",
			run: (body) =>
				workspaceRegistryApi.knowledgeMutate(slug, "propose", body),
			initial:
				'{"predicate":"business_description","value":"","source":{"kind":"user","ref":""},"validFrom":"2026-01-01","certainty":"confirmed"}',
			help: "Foreslår én kildeunderbygget, effektivt dateret assertion.",
		},
		{
			permission: "company.knowledge.manage",
			label: "Review assertion",
			run: (body) => workspaceRegistryApi.knowledgeMutate(slug, "review", body),
			initial: '{"assertionId":"","decision":"approved","reason":""}',
			help: "Godkender eller afviser én eksisterende assertion.",
		},
		{
			permission: "company.knowledge.manage",
			label: "Supersedér assertion",
			run: (body) =>
				workspaceRegistryApi.knowledgeMutate(slug, "supersede", body),
			initial:
				'{"assertionId":"","replacement":{"predicate":"business_description","value":"","source":{"kind":"user","ref":""},"validFrom":"2026-01-01"}}',
			help: "Erstatter kun godkendt viden via en ny, reviewbar assertion.",
		},
	];
	const ownershipActions: Action[] = [
		{
			permission: "company.ownership.manage",
			label: "Foreslå snapshot",
			run: (body) =>
				workspaceRegistryApi.ownershipMutate(slug, "propose", body),
			initial:
				'{"source":"registry","observedAt":"2026-01-01T00:00:00.000Z","facts":[]}',
			help: "Gemmer en kilde-hashet legal observation og deterministic diff.",
		},
		{
			permission: "company.ownership.manage",
			label: "Review snapshot",
			run: (body) => workspaceRegistryApi.ownershipMutate(slug, "review", body),
			initial: '{"snapshotId":"","decision":"approved"}',
			help: "Godkender eller afviser den konkrete snapshot uden at ændre facts.",
		},
		{
			permission: "company.ownership.manage",
			label: "Apply eksakt diff",
			run: (body) => workspaceRegistryApi.ownershipMutate(slug, "apply", body),
			initial: '{"snapshotId":"","snapshotHash":"","diffHash":""}',
			help: "Anvender kun den eksakt reviewede hash; adgang til alle endpoints genkontrolleres.",
		},
	];
	return (
		<section
			data-cockpit-page="workspace-register"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Parter og governance-records"
				actions={
					<Button
						variant="secondary"
						type="button"
						onClick={reload}
						xstyle={[cockpitStyles.pageButtonComposition]}
					>
						Opdater
					</Button>
				}
			>
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Workspace
					</p>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Kun relationer, der er synlige i denne virksomhed, vises. Originale
						records ændres aldrig her.
					</p>
				</div>
			</PageHeader>
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<section
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}
					>
						Canonical parter
					</h3>
					{parties.data?.rows.length ? (
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{parties.data.rows.map((p) => (
								<li
									id={`party-${p.partyId}`}
									key={p.partyId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<strong
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{p.name}
									</strong>{" "}
									<span
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{p.kind} · {p.roles.map((r) => r.role).join(", ")}
									</span>
								</li>
							))}
						</ul>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen synlige parter endnu.
						</p>
					)}
					<details
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<summary {...stylex.props(cockpitStyles.summaryComposition)}>
							Administrér parter
						</summary>
						{partyActions.map((action) => (
							<ActionForm key={action.label} action={action} onDone={reload} />
						))}
					</details>
				</section>
				<section
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}
					>
						Corporate records
					</h3>
					{records.data?.rows.length ? (
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{records.data.rows.map((r) => (
								<li
									key={r.recordId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<strong
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{r.filename}
									</strong>{" "}
									<span
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{r.type} · SHA-256 {r.sha256.slice(0, 12)}…
									</span>
								</li>
							))}
						</ul>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen synlige governance-records endnu.
						</p>
					)}
					<details
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<summary {...stylex.props(cockpitStyles.summaryComposition)}>
							Indlæs og vedligehold records
						</summary>
						<RecordIngest slug={slug} onDone={reload} />
						{recordActions.map((action) => (
							<ActionForm key={action.label} action={action} onDone={reload} />
						))}
					</details>
				</section>
				<section
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}
					>
						Virksomhedskontekst
					</h3>
					{context?.conflicts.length ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Konflikt kræver review: {context.conflicts.join(", ")}
						</p>
					) : null}
					{context?.assertions.length ? (
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{context.assertions.map((a) => (
								<li
									key={a.assertionId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<strong
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{a.predicate}
									</strong>{" "}
									<span
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{a.reviewState} · {a.source.kind}
									</span>
								</li>
							))}
						</ul>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen godkendt, kildeunderbygget kontekst endnu.
						</p>
					)}
					<details
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<summary {...stylex.props(cockpitStyles.summaryComposition)}>
							Foreslå, review og supersedér
						</summary>
						{knowledgeActions.map((action) => (
							<ActionForm key={action.label} action={action} onDone={reload} />
						))}
					</details>
				</section>
				<section
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}
					>
						Ejer- og kontrolforhold
					</h3>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Pr. dato{" "}
						<Input
							aria-label="Ownership as-of"
							type="date"
							value={asOf}
							onChange={(e) => setAsOf(e.target.value)}
							xstyle={[cockpitStyles.pageInputComposition]}
						/>
					</label>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{ownership.data?.partial
							? "Delvist synlige relationer; skjulte endpoints og antal vises ikke."
							: (ownership.data?.consolidation?.reason ??
								"Ownership-forhold kan ikke vises med den aktuelle adgang.")}
					</p>
					{ownership.data?.facts?.length ? (
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{ownership.data.facts.map((fact, index) => (
								<li
									key={index}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<code
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}
									>
										{fact.owner.kind === "company"
											? fact.owner.companySlug
											: fact.owner.partyId}
									</code>{" "}
									→{" "}
									<code
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}
									>
										{fact.ownedCompanySlug}
									</code>{" "}
									·{" "}
									{fact.economicBasisPoints != null
										? `${fact.economicBasisPoints / 100}%`
										: "interval"}{" "}
									· {fact.controlType}
								</li>
							))}
						</ul>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen synlige, godkendte facts.
						</p>
					)}
					<details
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<summary {...stylex.props(cockpitStyles.summaryComposition)}>
							Forslag, review og apply
						</summary>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Historik er append-only. Uunderstøttede projektioner vises aldrig
							som konsolidering; brug kun den eksplicitte serverstatus.
						</p>
						{ownershipActions.map((action) => (
							<ActionForm key={action.label} action={action} onDone={reload} />
						))}
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{history.data?.history?.map((item) => (
								<li
									key={item.snapshotId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<code
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}
									>
										{item.snapshotId}
									</code>{" "}
									· {item.state} ·{" "}
									<code
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}
									>
										{item.snapshotHash.slice(0, 12)}…
									</code>
								</li>
							))}
						</ul>
					</details>
				</section>
			</div>
		</section>
	);
}
