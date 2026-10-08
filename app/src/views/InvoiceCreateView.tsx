import * as stylex from "@stylexjs/stylex";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { InvoiceIssueForm } from "../components/InvoiceIssueModal";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";
import { listReturnTo } from "./workflow-navigation";

/** Full-page create uses exactly the same preview/issue core as legacy callers. */
export function InvoiceCreateView() {
	const { slug = "" } = useParams();
	const { can } = useCapabilities(slug);
	const { year, setYear } = useCompanyYear();
	const [params] = useSearchParams();
	const navigate = useNavigate();
	const state = useAsync(
		(signal) => api.invoices(slug, year, { signal }),
		[slug, year],
	);
	if (state.loading && !state.data)
		return <Loading label="Henter fakturaoplysninger…" />;
	if (state.error)
		return <ErrorState message={state.error} onRetry={state.reload} />;
	const invoices = state.data!;
	const returnTo = listReturnTo(
		slug,
		"fakturaer",
		params.get("returnTo"),
		invoices.selectedYear,
	);
	return (
		<section
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Ny faktura"
				description={`${invoices.company.name} · Regnskabsår ${invoices.selectedYear}`}
				actions={
					<ButtonLink
						to={returnTo}
						variant={"secondary"}
						xstyle={[cockpitStyles.statementBtnComposition2]}
					>
						Tilbage til fakturaer
					</ButtonLink>
				}
			/>
			<CompanyNav
				slug={slug}
				years={invoices.fiscalYears}
				selectedYear={invoices.selectedYear}
				onYearChange={setYear}
			/>
			{!can("company.draft.write") ? (
				<div
					role="status"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Du kan ikke udstede fakturaer
					</h2>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Din rolle giver adgang til at læse virksomhedens oplysninger.
					</p>
				</div>
			) : invoices.archived ? (
				<div
					role="status"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Regnskabsåret er arkiveret
					</h2>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Der kan ikke udstedes fakturaer i {invoices.selectedYear}. Vælg et
						aktivt regnskabsår.
					</p>
				</div>
			) : (
				<InvoiceIssueForm
					key={`${slug}:${invoices.selectedYear}`}
					slug={slug}
					year={invoices.selectedYear}
					presentation="page"
					onIssued={() => undefined}
					onClose={() => navigate(returnTo)}
				/>
			)}
		</section>
	);
}
