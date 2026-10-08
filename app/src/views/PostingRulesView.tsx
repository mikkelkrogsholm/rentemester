import * as stylex from "@stylexjs/stylex";
import { useParams } from "react-router-dom";
import { PageState } from "../components/CockpitPrimitives";
import { PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";

/** A deliberately separate operational view; legal source material remains at /lovgrundlag. */
export function PostingRulesView() {
	const { slug = "" } = useParams();
	const state = useAsync(
		(signal) => api.postingRules(slug, { signal }),
		[slug],
	);
	if (state.loading && !state.data)
		return <PageState kind="loading" title="Henter posteringsregler" />;
	if (state.error)
		return (
			<PageState
				kind="error"
				title="Posteringsregler kunne ikke hentes"
				onRetry={state.reload}
			>
				{state.error}
			</PageState>
		);
	return (
		<section
			data-cockpit-page="posting-rules"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader title="Posteringsregler">
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Selskabslokale forslag og godkendte versioner — adskilt fra
						Lovgrundlag.
					</p>
				</div>
			</PageHeader>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Detaljer og handlinger sker med hash, begrundelse og eksplicit
				bekræftelse via API/CLI/MCP.
			</p>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.tableScroll,
				)}
				data-ui="table-scroll"
			>
				<table
					aria-label="Posteringsregler"
					{...stylex.props(cockpitStyles.statementTableScrollTableComposition)}
				>
					<thead
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.responsiveTableThead,
						)}
					>
						<tr
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTr,
							)}
						>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition3,
								)}
							>
								Regel
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition3,
								)}
							>
								Version
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition3,
								)}
							>
								Proveniens
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition3,
								)}
							>
								Evidens
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.responsiveTableTbody,
						)}
					>
						{state.data!.length ? (
							state.data!.map((rule) => (
								<tr
									key={`${rule.ruleId}-${rule.version}`}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}
								>
									<td
										data-label="Regel"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition4,
										)}
									>
										{rule.ruleId}
									</td>
									<td
										data-label="Version"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition4,
										)}
									>
										v{rule.version}
									</td>
									<td
										data-label="Proveniens"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition4,
										)}
									>
										{rule.provenance}
									</td>
									<td
										data-label="Evidens"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition4,
										)}
									>
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{rule.payloadHash.slice(0, 12)}…
										</code>
									</td>
								</tr>
							))
						) : (
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<td
									colSpan={4}
									{...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition5,
									)}
								>
									Ingen posteringsregler endnu.
								</td>
							</tr>
						)}
					</tbody>
				</table>
			</div>
		</section>
	);
}
