import * as stylex from "@stylexjs/stylex";
import { useParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { ApiError, api } from "../lib/api";
import { useAsync } from "../lib/useAsync";

const styles = stylex.create({
	body: { whiteSpace: "pre-wrap", overflowWrap: "anywhere", minWidth: 0 },
});
/** Protected product reference; Markdown is displayed as escaped text. */
export function KnowledgePageView() {
	const { slug, pageId = "" } = useParams();
	const state = useAsync(
		(signal) => api.knowledgePage(pageId, slug, { signal }),
		[slug, pageId],
	);
	if (
		state.failure instanceof ApiError &&
		[403, 404].includes(state.failure.status)
	)
		return (
			<ErrorState
				message="Referencen er ikke tilgængelig."
				onRetry={state.reload}
			/>
		);
	if (state.loading && !state.data)
		return <Loading label="Henter viden og playbook…" />;
	if (!state.data)
		return (
			<ErrorState
				message={state.error ?? "Referencen er ikke tilgængelig."}
				onRetry={state.reload}
			/>
		);
	const { page } = state.data;
	return (
		<section
			data-cockpit-page="knowledge-page"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title={page.title}
				description={`${page.scope.kind === "company" ? "Selskabets viden" : "Workspaceviden"} · version ${page.version}`}
			/>
			{state.error && (
				<p
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Senest hentede version vises. {state.error}
				</p>
			)}
			<article
				aria-label="Viden og playbook"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
					styles.body,
				)}
			>
				{page.bodyMarkdown}
			</article>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Reference: {page.pageId} · Grundlag: {page.provenance.kind}:{" "}
				{page.provenance.ref}
			</p>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Gyldig fra {page.effectiveFrom}
				{page.effectiveToExclusive
					? ` til ${page.effectiveToExclusive} (eksklusiv)`
					: ""}
			</p>
		</section>
	);
}
