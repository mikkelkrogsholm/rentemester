import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useParams } from "react-router-dom";
import { PageState } from "../components/CockpitPrimitives";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Button, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import type { AccountingApprovalPolicy } from "../lib/api/accounting-approval-policy";
import { useAsync } from "../lib/useAsync";

const LABEL: Record<AccountingApprovalPolicy["reviewMode"], string> = {
	independent_reviewer: "Uafhængig reviewer",
	sole_authorized_bookkeeper: "Autoriseret bogholder",
};

export function AccountingApprovalPolicyView() {
	const { slug = "" } = useParams();
	const state = useAsync(() => api.accountingApprovalPolicy(slug), [slug]);
	const [pending, setPending] = useState<
		AccountingApprovalPolicy["reviewMode"] | null
	>(null);
	const [error, setError] = useState<string | null>(null);
	if (state.loading)
		return <PageState kind="loading" title="Henter godkendelsespolitik" />;
	if (state.error)
		return (
			<PageState
				kind="error"
				title="Godkendelsespolitik kunne ikke hentes"
				onRetry={state.reload}
			>
				{state.error}
			</PageState>
		);
	const policy = state.data;
	async function save() {
		if (!pending) return;
		setError(null);
		try {
			await api.setAccountingApprovalPolicy(
				slug,
				pending,
				policy?.eventHash ?? null,
			);
			state.reload();
		} catch (cause) {
			setError(
				cause instanceof Error
					? cause.message
					: "Politikken kunne ikke ændres.",
			);
			throw cause;
		}
	}
	return (
		<section
			data-cockpit-page="approval-policy"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Godkendelsespolitik"
				description="Bestemmer hvem der kan færdiggøre kontrollerede køb, batches og kladder. Den ændrer aldrig adgang eller bogføring i sig selv."
			/>
			{error && (
				<div
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{error}
					</p>
				</div>
			)}
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
					Aktiv politik
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{policy ? LABEL[policy.reviewMode] : LABEL.independent_reviewer}
					</strong>
				</p>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{policy
						? `Version ${policy.version}. `
						: "Ingen særregel er sat; fail-safe standarden gælder. "}
					{policy ? (
						<code
							title={policy.eventHash}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.code,
							)}
						>
							{policy.eventHash.slice(0, 12)}…
						</code>
					) : (
						""
					)}
				</p>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<Button
						requiredPermission="company.admin"
						type="button"
						disabled={
							(policy?.reviewMode ?? "independent_reviewer") ===
							"independent_reviewer"
						}
						onClick={() => setPending("independent_reviewer")}
						variant={"secondary"}
						xstyle={[cockpitStyles.statementBtnComposition]}
					>
						Kræv uafhængig reviewer
					</Button>
					<Button
						requiredPermission="company.admin"
						type="button"
						disabled={policy?.reviewMode === "sole_authorized_bookkeeper"}
						onClick={() => setPending("sole_authorized_bookkeeper")}
						xstyle={[cockpitStyles.statementBtnComposition]}
					>
						Tillad autoriseret bogholder
					</Button>
				</div>
			</section>
			{pending && (
				<ConfirmDialog
					title="Ændr godkendelsespolitik"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Ændringen opretter en ny, append-only policy-version. Eksisterende
							adgang, dokumentation, moms- og periodelåse ændres ikke.
						</p>
					}
					confirmLabel="Gem politik"
					onConfirm={save}
					onClose={() => setPending(null)}
				/>
			)}
		</section>
	);
}
