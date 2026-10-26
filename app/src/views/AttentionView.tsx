import * as stylex from "@stylexjs/stylex";
import { useNavigate, useParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import type { CompanyAttention } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const sourceLabel = {
	exception: "Undtagelse",
	"agent-proposal": "Agentforslag",
	readiness: "Lukkeparathed",
	workbench: "Bogføringskø",
} as const;

/** The human-facing view of the read-only #649 attention projection. */
export function AttentionView() {
	const { slug = "" } = useParams();
	const navigate = useNavigate();
	const state = useAsync<CompanyAttention>(() => api.attention(slug), [slug]);
	if (state.loading && !state.data)
		return (
			<section
				data-evidence-issue="649"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<PageHeader evidenceHeading title="Opgaver der kræver opmærksomhed" />
				<p
					data-evidence-status="loading"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Henter opgaver
				</p>
				<Loading label="Henter opgaver" />
			</section>
		);
	if (state.error) {
		const blocked = /\b403\b|forbudt|adgang/i.test(state.error);
		return (
			<section
				data-evidence-issue="649"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<PageHeader evidenceHeading title="Opgaver der kræver opmærksomhed" />
				<p
					data-evidence-status={blocked ? "warning-or-blocked" : "error"}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{blocked ? "Opgaver er blokeret" : "Opgaver kunne ikke hentes"}
				</p>
				<ErrorState
					message={
						blocked
							? "Du har ikke adgang til disse opgaver."
							: "Opgaver kunne ikke hentes"
					}
					onRetry={state.reload}
				/>
			</section>
		);
	}
	const attention = state.data!;
	return (
		<section
			data-cockpit-page="attention"
			data-evidence-issue="649"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				evidenceHeading
				title="Opgaver der kræver opmærksomhed"
				actions={
					<ButtonLink
						variant="secondary"
						to={`/companies/${slug}/opgaver`}
						xstyle={[cockpitStyles.aComposition]}
					>
						Planlæg i Opgaver
					</ButtonLink>
				}
				description={`${attention.company.name} · én samlet liste over det, der skal afklares.`}
			/>
			{attention.status === "clear" ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<p
						data-evidence-status="empty"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Ingen opgaver kræver opmærksomhed
					</p>
				</div>
			) : (
				<>
					<p
						data-evidence-status="normal"
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{attention.count} forhold kræver opmærksomhed
					</p>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}
					>
						Prioriteret opgaveliste
					</h3>
					<ol
						aria-label="Prioriteret opgaveliste"
						data-evidence-data
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{attention.items.map((item, index) => (
							<li
								key={item.id}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.card,
								)}
							>
								<div
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<h3
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.h3,
										)}
									>
										{item.title}
									</h3>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{item.reason}
									</p>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{sourceLabel[item.source]}
									</p>
								</div>
								<div
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<Button
										type="button"
										data-evidence-core-action={index === 0 ? true : undefined}
										onClick={() =>
											navigate(`/companies/${slug}/${item.destination}`)
										}
										xstyle={[cockpitStyles.attentionViewBtnComposition]}
									>
										{index === 0 ? "Åbn næste opgave" : "Åbn opgave"}
									</Button>
									<details
										data-evidence-progressive={index === 0 ? true : undefined}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<summary
											{...stylex.props(cockpitStyles.summaryComposition)}
										>
											Se grundlag for opgaven
										</summary>
										<dl
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<dt
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												Kilde
											</dt>
											<dd
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{item.sourceIdentity}
											</dd>
											<dt
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												Aktør
											</dt>
											<dd
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{item.actor ?? "Ikke angivet"}
											</dd>
											<dt
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												Teknisk grundlag
											</dt>
											<dd
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
													{JSON.stringify(item.evidence)}
												</code>
											</dd>
										</dl>
									</details>
								</div>
							</li>
						))}
					</ol>
				</>
			)}
		</section>
	);
}
