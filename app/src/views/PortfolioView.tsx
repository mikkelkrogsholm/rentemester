import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Portfolio overview — the workspace-level landing page.
//
// A cross-company roll-up strip answers "how is the whole portfolio doing",
// and one card per company shows the headline health an owner judges a
// company on. Companies that need attention sort to the top and are flagged.

import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { CompanyCard } from "../components/CompanyCard";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner, sortByAttention } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { Onboarding } from "./Onboarding";

export function PortfolioView() {
	const navigate = useNavigate();
	const state = useAsync((signal) => api.portfolio(undefined, { signal }), []);
	useEffect(() => {
		if (state.data?.companies.length === 1) {
			navigate(`/companies/${state.data.companies[0]!.slug}`, {
				replace: true,
			});
		}
	}, [navigate, state.data]);

	if (state.loading) return <Loading label="Henter portefølje…" />;
	if (state.error)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const portfolio = state.data!;

	// The server has already filtered this list to the authenticated member's
	// visible companies. Do not reconstruct membership in the browser.
	// First run: an empty workspace drops straight into onboarding.
	if (portfolio.companies.length === 0) {
		return <Onboarding onCreated={(slug) => navigate(`/companies/${slug}`)} />;
	}

	if (portfolio.companies.length === 1)
		return <Loading label="Åbner virksomhedsoverblik…" />;

	const ordered = sortByAttention(portfolio.companies);
	const needAttention = ordered.filter(
		(c) =>
			!c.archived &&
			(c.ledgerMissing ||
				!c.auditChainOk ||
				c.resultat < 0 ||
				c.attentionStatus === "requires-attention" ||
				(c.vat !== null && c.vat.payable > 0 && c.vat.daysRemaining <= 30)),
	).length;

	const { rollup } = portfolio;

	return (
		<section
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Portefølje"
				actions={
					<>
						<ButtonLink
							variant="secondary"
							to="/opgaver"
							xstyle={[cockpitStyles.aComposition]}
						>
							Opgaver på tværs
						</ButtonLink>
						<ButtonLink
							to="/companies/new"
							xstyle={[cockpitStyles.aComposition]}
						>
							Tilføj virksomhed
						</ButtonLink>
					</>
				}
			>
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
						{portfolio.companyCount} virksomhed
						{portfolio.companyCount === 1 ? "" : "er"} · {needAttention} kræver
						opmærksomhed · pr. {portfolio.asOf}
					</p>
				</div>
			</PageHeader>

			{rollup && (
				<div
					role="group"
					aria-label="Tværgående overblik"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rollupStrip,
					)}
				>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rollupCell,
							!(rollup.resultat < 0) && cockpitStyles.rollupCellPos,
							rollup.resultat < 0 && cockpitStyles.rollupCellNeg,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupLabel,
							)}
						>
							Samlet resultat
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupValue,
								rollup.resultat < 0 && cockpitStyles.rollupCellNegRollupValue,
							)}
						>
							{formatKroner(rollup.resultat)}
						</span>
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rollupCell,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupLabel,
							)}
						>
							Samlet likviditet
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupValue,
							)}
						>
							{rollup.liquidity === null ? "—" : formatKroner(rollup.liquidity)}
						</span>
						{!rollup.liquidityComplete && (
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Ufuldstændig — kontrollér Bank
							</span>
						)}
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rollupCell,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupLabel,
							)}
						>
							{rollup.vatPayable < 0 ? "Moms til gode" : "Moms at betale"}
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupValue,
							)}
						>
							{formatKroner(rollup.vatPayable)}
						</span>
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rollupCell,
							rollup.openTaskCount > 0 && cockpitStyles.rollupCellWarn,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupLabel,
							)}
						>
							Åbne opgaver
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rollupCellRollupValue,
								rollup.openTaskCount > 0 &&
									cockpitStyles.rollupCellWarnRollupValue,
							)}
						>
							{rollup.openTaskCount}
						</span>
					</div>
				</div>
			)}

			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.companyGrid,
				)}
			>
				{ordered.map((c) => (
					<CompanyCard key={c.slug} company={c} />
				))}
			</div>
		</section>
	);
}
