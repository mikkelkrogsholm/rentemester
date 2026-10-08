import * as stylex from "@stylexjs/stylex";
import { type FormEvent, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, PageHeader, Select } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { workspaceInboxApi } from "../lib/api/workspace-inbox";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

/** One deliberately small inbox: ingest → inspect → explicitly assign → handoff. */
export function WorkspaceInboxView() {
	const { slug = "" } = useParams();
	const data = useAsync(
		(signal) => workspaceInboxApi.list(slug, { signal }),
		[slug],
	);
	const companies = useAsync((signal) => api.companies({ signal }), []);
	const [file, setFile] = useState<File>();
	const [target, setTarget] = useState("");
	const [message, setMessage] = useState<string>();
	const ingestKey = useRef<string>();
	const outcome = useMutationOutcome(() => data.reload());
	useUnsavedChanges(Boolean(file));
	const reload = () => data.reload();
	async function ingest(event: FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		if (!file) {
			setMessage("Vælg en fil.");
			return;
		}
		try {
			if (!ingestKey.current) ingestKey.current = crypto.randomUUID();
			const encoded = await new Promise<string>((resolve, reject) => {
				const r = new FileReader();
				r.onerror = () => reject(new Error("Filen kunne ikke læses."));
				r.onload = () => resolve(String(r.result).split(",")[1] ?? "");
				r.readAsDataURL(file);
			});
			await outcome.run(() =>
				workspaceInboxApi.ingest(slug, {
					idempotencyKey: ingestKey.current,
					bytesBase64: encoded,
					filename: file.name,
					mimeType: file.type || "application/octet-stream",
					transport: "upload",
					receivedAt: new Date().toISOString(),
					metadata: {},
					candidates: [],
				}),
			);
			setFile(undefined);
			ingestKey.current = undefined;
			setMessage("Kilden er lagt i workspace-indbakken uden bogføring.");
			reload();
		} catch (error) {
			setMessage(
				error instanceof Error ? error.message : "Indlæsning fejlede.",
			);
		}
	}
	async function act(sourceId: string, action: "assign" | "complete") {
		if (outcome.isBlocked()) return;
		try {
			if (!target) {
				setMessage("Vælg målvirksomheden efter gennemgang.");
				return;
			}
			await outcome.run(() =>
				workspaceInboxApi[action](slug, sourceId, target),
			);
			setMessage(
				action === "assign"
					? "Ruting er godkendt. Håndoff er stadig et separat trin."
					: "Kilden er overdraget én gang til virksomhedens dokumentflow.",
			);
			reload();
		} catch (error) {
			setMessage(
				error instanceof Error ? error.message : "Handlingen fejlede.",
			);
		}
	}
	if (data.loading && !data.data)
		return <Loading label="Henter workspace-indbakke…" />;
	if (data.error) return <ErrorState message={data.error} onRetry={reload} />;
	return (
		<section
			data-cockpit-page="workspace-inbox"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			{outcome.feedback}
			<PageHeader
				title="Fælles dokumentindbakke"
				actions={
					<>
						<Button
							variant="secondary"
							type="button"
							onClick={reload}
							xstyle={[cockpitStyles.pageButtonComposition]}
						>
							Opdater
						</Button>
					</>
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
						Kilder ligger uden for hovedbogen, indtil en autoriseret bruger har
						valgt virksomhed og fuldført overdragelsen.
					</p>
				</div>
			</PageHeader>
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
					Indlæs kilde
				</h3>
				<form
					onSubmit={(event) => void ingest(event)}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<Input
						disabled={outcome.blocked}
						aria-label="Inbox-fil"
						type="file"
						onChange={(event) => setFile(event.target.files?.[0])}
						xstyle={[cockpitStyles.pageInputComposition]}
					/>
					<Button
						requiredPermission="company.documents.upload"
						variant="secondary"
						type="submit"
						disabled={outcome.blocked || !file}
						xstyle={[cockpitStyles.pageBtnComposition2]}
					>
						Indlæs til indbakke
					</Button>
				</form>
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
					Afventer review
				</h3>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Målvirksomhed efter gennemgang
					<Select
						disabled={outcome.blocked}
						aria-label="Inbox målvirksomhed"
						value={target}
						onChange={(event) => setTarget(event.target.value)}
						xstyle={[cockpitStyles.pageSelectComposition]}
					>
						<option
							value=""
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Vælg virksomhed
						</option>
						{companies.data
							?.filter((company) => !company.archived)
							.map((company) => (
								<option
									key={company.slug}
									value={company.slug}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{company.name}
								</option>
							))}
					</Select>
				</label>
				{data.data?.rows.length ? (
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{data.data.rows.map((source) => (
							<li
								key={source.sourceId}
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
									{source.filename}
								</strong>{" "}
								<span
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.muted,
									)}
								>
									{source.transport} · SHA-256 {source.sha256.slice(0, 12)}… ·{" "}
									{(source.exception?.code ??
										source.assignments
											.map((item) => `${item.companySlug}: ${item.state}`)
											.join(", ")) ||
										"kræver review"}
								</span>
								<div
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.modalActions,
									)}
								>
									<Button
										disabled={outcome.blocked}
										requiredPermission="company.documents.upload"
										variant="secondary"
										type="button"
										onClick={() => void act(source.sourceId, "assign")}
										xstyle={[cockpitStyles.pageButtonComposition]}
									>
										Godkend ruting
									</Button>
									<Button
										disabled={outcome.blocked}
										requiredPermission="company.documents.upload"
										variant="secondary"
										type="button"
										onClick={() => void act(source.sourceId, "complete")}
										xstyle={[cockpitStyles.pageButtonComposition]}
									>
										Fuldfør handoff
									</Button>
								</div>
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
						Ingen synlige kilder.
					</p>
				)}
			</section>
			{message && (
				<Banner
					kind={
						message.includes("fejl") || message.includes("Angiv")
							? "error"
							: "success"
					}
				>
					{message}
				</Banner>
			)}
		</section>
	);
}
